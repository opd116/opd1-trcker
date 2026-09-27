//! Tests for `opd1_core::stats` — pure stats math (Task 4).
//!
//! `act_label`/`act_full_name` are exercised against real
//! `valorant-api.com/v1/seasons` data (fetched 2026-09-27), trimmed to
//! `tests/fixtures/seasons.json`. This file's `load_acts` helper mirrors
//! the conversion Task 5's asset-fetching code will really do (resolve
//! each `EAresSeasonType::Act` entry's `parentUuid` into the parent
//! episode's `displayName`) — but it is test-only glue: `opd1-core` itself
//! never parses valorant-api JSON or ISO-8601 timestamps.

use std::collections::HashMap;
use std::ops::Range;

use opd1_core::stats::{
    act_full_name, act_label, hs_pct, kd, peak, prev_act_tier, since_act_end, split_sessions,
    winrate, Act, Peak, SeasonInfo, SeasonalMap, DEFAULT_SESSION_GAP_MS,
};
use serde::Deserialize;

const SIX_HOURS_MS: i64 = 6 * 60 * 60 * 1000;

// ---------------------------------------------------------------------
// Fixture loading
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawSeason {
    uuid: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "endTime")]
    end_time: String,
    #[serde(rename = "parentUuid")]
    parent_uuid: Option<String>,
}

/// Hand-rolled RFC3339 (`YYYY-MM-DDTHH:MM:SSZ`, always UTC, no fractional
/// seconds — the exact shape valorant-api.com uses) to epoch millis, via
/// Howard Hinnant's `days_from_civil` (proleptic Gregorian, no leap
/// seconds). Verified against `date -u -d ... +%s` for every timestamp
/// used below before this file was written.
fn parse_iso_ms(s: &str) -> i64 {
    let bytes = s.as_bytes();
    assert_eq!(bytes.len(), 20, "expected YYYY-MM-DDTHH:MM:SSZ, got {s}");
    let year: i64 = s[0..4].parse().unwrap();
    let month: i64 = s[5..7].parse().unwrap();
    let day: i64 = s[8..10].parse().unwrap();
    let hour: i64 = s[11..13].parse().unwrap();
    let min: i64 = s[14..16].parse().unwrap();
    let sec: i64 = s[17..19].parse().unwrap();

    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    let secs = days * 86400 + hour * 3600 + min * 60 + sec;
    secs * 1000
}

fn load_raw() -> Vec<RawSeason> {
    let raw = include_str!("fixtures/seasons.json");
    serde_json::from_str(raw).expect("fixture parses")
}

/// Every `Act` in the fixture, sorted ascending by `start_ms` — the shape
/// `peak`/`prev_act_tier` expect an `ordered_acts` caller to hand them.
fn load_acts() -> Vec<Act> {
    let raw = load_raw();
    let by_uuid: HashMap<&str, &RawSeason> = raw.iter().map(|s| (s.uuid.as_str(), s)).collect();

    let mut acts: Vec<Act> = raw
        .iter()
        .filter(|s| s.kind.as_deref() == Some("EAresSeasonType::Act"))
        .map(|s| {
            let parent = by_uuid[s.parent_uuid.as_deref().expect("act has a parent")];
            Act {
                id: s.uuid.clone(),
                name: s.display_name.clone(),
                episode_name: parent.display_name.clone(),
                start_ms: parse_iso_ms(&s.start_time),
                end_ms: parse_iso_ms(&s.end_time),
            }
        })
        .collect();

    acts.sort_by_key(|a| a.start_ms);
    acts
}

fn act_by_label(acts: &[Act], label: &str) -> Act {
    acts.iter()
        .find(|a| act_label(a) == label)
        .unwrap_or_else(|| panic!("no act labeled {label}"))
        .clone()
}

// ---------------------------------------------------------------------
// kd
// ---------------------------------------------------------------------

#[test]
fn kd_zero_deaths() {
    assert_eq!(kd(12, 0), 12.0);
}

#[test]
fn kd_normal_ratio() {
    assert_eq!(kd(10, 4), 2.5);
}

// ---------------------------------------------------------------------
// hs_pct — fractions (0..1), never percentages (ruling R5)
// ---------------------------------------------------------------------

#[test]
fn hs_pct_computes_fraction() {
    assert_eq!(hs_pct(25, 50, 25), Some(0.25));
}

#[test]
fn hs_pct_none_when_no_shots() {
    assert_eq!(hs_pct(0, 0, 0), None);
}

// ---------------------------------------------------------------------
// winrate — fractions (0..1), never percentages (ruling R5)
// ---------------------------------------------------------------------

#[test]
fn winrate_zero_games_none() {
    assert_eq!(winrate(0, 0), None);
}

#[test]
fn winrate_computes_fraction() {
    assert_eq!(winrate(6, 20), Some(0.3));
}

// ---------------------------------------------------------------------
// peak
// ---------------------------------------------------------------------

fn season_with_tier_win(tier: u8, wins: u32) -> SeasonInfo {
    let mut wins_by_tier = HashMap::new();
    wins_by_tier.insert(tier, wins);
    SeasonInfo {
        competitive_tier: tier,
        wins_by_tier,
        number_of_wins: wins,
        number_of_games: wins,
    }
}

#[test]
fn peak_across_seasons() {
    let acts = load_acts();
    let e8a3 = act_by_label(&acts, "E8 A3");
    let v26a4 = act_by_label(&acts, "V26 A4");
    let v26a5 = act_by_label(&acts, "V26 A5");

    let mut seasons: SeasonalMap = HashMap::new();
    seasons.insert(e8a3.id.clone(), season_with_tier_win(18, 3));
    seasons.insert(v26a4.id.clone(), season_with_tier_win(22, 5));
    seasons.insert(v26a5.id.clone(), season_with_tier_win(15, 2));

    let peak = peak(&seasons, &acts).expect("has a peak");
    assert_eq!(
        peak,
        Peak {
            tier: 22,
            season_id: v26a4.id,
        }
    );
}

#[test]
fn peak_normalizes_pre_e5_tiers() {
    let acts = load_acts();
    let e4a3 = act_by_label(&acts, "E4 A3"); // started before E5A1 -> old tier table
    let e8a3 = act_by_label(&acts, "E8 A3"); // started after E5A1 -> new tier table

    let mut seasons: SeasonalMap = HashMap::new();
    // Old-table Immortal 1 (21) normalizes to new-table 24 (Ascendant
    // inserted), which beats E8A3's raw (already-new-table) 22.
    seasons.insert(e4a3.id.clone(), season_with_tier_win(21, 1));
    seasons.insert(e8a3.id.clone(), season_with_tier_win(22, 1));

    let peak = peak(&seasons, &acts).expect("has a peak");
    assert_eq!(peak.tier, 24);
    assert_eq!(peak.season_id, e4a3.id);
}

#[test]
fn peak_none_when_no_ranked_wins() {
    let acts = load_acts();
    let e8a3 = act_by_label(&acts, "E8 A3");
    let mut seasons: SeasonalMap = HashMap::new();
    // Tier 2 is unranked/unused per ruling R6 — ignored even with wins.
    seasons.insert(e8a3.id.clone(), season_with_tier_win(2, 4));

    assert_eq!(peak(&seasons, &acts), None);
}

// ---------------------------------------------------------------------
// prev_act_tier
// ---------------------------------------------------------------------

#[test]
fn prev_act_tier_returns_previous_act() {
    let acts = load_acts();
    // Caller filters to "acts that have started"; the last of those is
    // the current act (V26 A5 here), so the previous act is V26 A4.
    let v26a4 = act_by_label(&acts, "V26 A4");

    let mut seasons: SeasonalMap = HashMap::new();
    seasons.insert(
        v26a4.id.clone(),
        SeasonInfo {
            competitive_tier: 19,
            wins_by_tier: HashMap::new(),
            number_of_wins: 5,
            number_of_games: 10,
        },
    );

    assert_eq!(prev_act_tier(&seasons, &acts), Some(19));
}

#[test]
fn prev_act_tier_none_when_unranked() {
    let acts = load_acts();
    let v26a4 = act_by_label(&acts, "V26 A4");

    let mut seasons: SeasonalMap = HashMap::new();
    seasons.insert(
        v26a4.id.clone(),
        SeasonInfo {
            competitive_tier: 0,
            wins_by_tier: HashMap::new(),
            number_of_wins: 0,
            number_of_games: 0,
        },
    );

    assert_eq!(prev_act_tier(&seasons, &acts), None);
}

#[test]
fn prev_act_tier_normalizes_pre_e5_tier() {
    let acts = load_acts();
    let e4a3 = act_by_label(&acts, "E4 A3");
    let e5a1 = act_by_label(&acts, "E5 A1");
    // As of E5A1, only these two acts have started; E5A1 is current.
    let ordered = vec![e4a3.clone(), e5a1];

    let mut seasons: SeasonalMap = HashMap::new();
    seasons.insert(
        e4a3.id.clone(),
        SeasonInfo {
            competitive_tier: 23, // old-table Immortal 3 -> normalizes to 26
            wins_by_tier: HashMap::new(),
            number_of_wins: 5,
            number_of_games: 10,
        },
    );

    assert_eq!(prev_act_tier(&seasons, &ordered), Some(26));
}

// ---------------------------------------------------------------------
// act_label / act_full_name
// ---------------------------------------------------------------------

#[test]
fn act_label_new_and_old_naming() {
    let acts = load_acts();
    assert_eq!(act_label(&act_by_label(&acts, "E8 A3")), "E8 A3");
    assert_eq!(act_label(&act_by_label(&acts, "V26 A5")), "V26 A5");
}

#[test]
fn act_full_name_for_hover() {
    let acts = load_acts();
    assert_eq!(
        act_full_name(&act_by_label(&acts, "E8 A3")),
        "EPISODE 8 ACT III"
    );
    assert_eq!(act_full_name(&act_by_label(&acts, "V26 A5")), "V26 ACT V");
}

// ---------------------------------------------------------------------
// since_act_end
// ---------------------------------------------------------------------

#[test]
fn since_act_end_none_for_current_act() {
    let acts = load_acts();
    let v26a5 = act_by_label(&acts, "V26 A5"); // 2026-08-19 .. 2026-10-14
    let now_ms = parse_iso_ms("2026-09-15T00:00:00Z"); // inside the act
    assert_eq!(since_act_end(&v26a5, now_ms), None);
}

#[test]
fn since_act_end_some_for_past_act() {
    let acts = load_acts();
    let v26a4 = act_by_label(&acts, "V26 A4"); // ends 2026-08-19
    let now_ms = parse_iso_ms("2026-08-20T00:00:00Z"); // one day after
    assert_eq!(since_act_end(&v26a4, now_ms), Some(86_400_000));
}

// ---------------------------------------------------------------------
// split_sessions
// ---------------------------------------------------------------------

#[test]
fn sessions_split_on_6h_gap() {
    let starts = [
        0,
        1_000,
        SIX_HOURS_MS + 2_000, // > 6h after the previous match -> new session
        SIX_HOURS_MS + 3_000,
    ];
    let ranges = split_sessions(&starts, SIX_HOURS_MS);
    assert_eq!(
        ranges,
        vec![Range { start: 0, end: 2 }, Range { start: 2, end: 4 }]
    );
}

#[test]
fn sessions_split_handles_empty_input() {
    let starts: [i64; 0] = [];
    assert_eq!(
        split_sessions(&starts, DEFAULT_SESSION_GAP_MS),
        Vec::<Range<usize>>::new()
    );
}

#[test]
fn sessions_default_gap_is_six_hours() {
    assert_eq!(DEFAULT_SESSION_GAP_MS, SIX_HOURS_MS);
}
