//! Pure stats math — no DB, no I/O, no Riot/valorant-api types. Task 5
//! converts Riot's raw responses into the plain input types here
//! ([`SeasonInfo`]/[`SeasonalMap`]/[`Act`]), per ruling R20.
//!
//! Fractions, not percentages (ruling R5): [`hs_pct`] and [`winrate`]
//! return `0.0..=1.0` — a UI formats `0.545` as `54.5%`, this module never
//! multiplies by 100.

use std::collections::HashMap;
use std::ops::Range;

/// Six hours in milliseconds — CLAUDE.md's "My Stats" session-split gap
/// ("A new session starts after a 6h gap with no comp match").
pub const DEFAULT_SESSION_GAP_MS: i64 = 6 * 60 * 60 * 1000;

/// The act whose start marks the old-vs-new competitive tier table split
/// (ruling R6): Ascendant was inserted at Episode 5 Act 1, shifting every
/// tier from Immortal 1 up by 3. Acts that *started* before this one are
/// on the old table; this act itself already uses the new one.
const E5A1_ID: &str = "67e373c7-48f7-b422-641b-079ace30b427";

// ---------------------------------------------------------------------
// R20 input types — plain data, no Riot/valorant-api shapes leak in here.
// ---------------------------------------------------------------------

/// One season's (= one act's) ranked record for a player, as Task 5
/// extracts it from `mmr/v1/players/{puuid}`'s
/// `QueueSkills.competitive.SeasonalInfoBySeasonID`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeasonInfo {
    /// The tier the player finished (or currently sits at) in this act —
    /// Riot's `CompetitiveTier`, not yet normalized (see [`peak`]/
    /// [`prev_act_tier`], which normalize it themselves).
    pub competitive_tier: u8,
    /// Tier -> wins recorded at that tier during the act — Riot's
    /// `WinsByTier`. The highest tier here with at least one win is the
    /// act's contribution to [`peak`].
    pub wins_by_tier: HashMap<u8, u32>,
    /// Wins in the act with placement games counted: Riot's
    /// `NumberOfWinsWithPlacements`, or `NumberOfWins` when the response
    /// has no such key. It is the numerator of the card's act win rate and
    /// `actWins`; `number_of_games` is the matching denominator.
    pub number_of_wins: u32,
    pub number_of_games: u32,
}

/// A player's ranked history, keyed by season/act id.
pub type SeasonalMap = HashMap<String, SeasonInfo>;

/// One competitive act, resolved from valorant-api.com's `/v1/seasons`
/// (Task 5's job — this crate never parses that JSON itself). `name` is
/// the act's own display name (`"ACT V"`); `episode_name` is its parent
/// episode's (`"V26"` or `"EPISODE 8"`) — see [`act_label`]/
/// [`act_full_name`] for how these become UI strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Act {
    pub id: String,
    pub name: String,
    pub episode_name: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// The highest ranked tier a player has ever won a game at, and which
/// season/act it happened in (ruling R7: the most recent one, if tied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peak {
    pub tier: u8,
    pub season_id: String,
}

// ---------------------------------------------------------------------
// kd / hs_pct / winrate
// ---------------------------------------------------------------------

/// Kills-per-death. Zero deaths just returns the kill count (ruling R5:
/// `kd(k, 0) == k as f32`), not `f32::INFINITY`.
pub fn kd(kills: i64, deaths: i64) -> f32 {
    if deaths == 0 {
        kills as f32
    } else {
        kills as f32 / deaths as f32
    }
}

/// Headshot fraction (0..1) of total shots landed. `None` when no shots
/// were recorded at all (e.g. a player who never fired).
pub fn hs_pct(headshots: i64, bodyshots: i64, legshots: i64) -> Option<f32> {
    let total = headshots + bodyshots + legshots;
    if total == 0 {
        None
    } else {
        Some(headshots as f32 / total as f32)
    }
}

/// Win fraction (0..1). `None` when no games have been played.
pub fn winrate(wins: i64, games: i64) -> Option<f32> {
    if games == 0 {
        None
    } else {
        Some(wins as f32 / games as f32)
    }
}

// ---------------------------------------------------------------------
// Tier normalization (ruling R6) — shared by peak/prev_act_tier
// ---------------------------------------------------------------------

/// Whether `act` started strictly before Episode 5 Act 1 — the cutoff for
/// the old (pre-Ascendant) tier table. `ordered_acts` is searched for
/// [`E5A1_ID`] rather than hardcoding its start time, so a caller's own
/// resolved timestamp for that act is always the source of truth.
fn started_before_e5a1(act: &Act, ordered_acts: &[Act]) -> bool {
    match ordered_acts.iter().find(|a| a.id == E5A1_ID) {
        Some(e5a1) => act.start_ms < e5a1.start_ms,
        None => false,
    }
}

/// Applies ruling R6: a tier of 21 or higher, reached in an act that
/// started before E5A1, moves up by 3 (Ascendant's insertion into the
/// tier table). Tiers below 21, and any tier from an act at or after
/// E5A1, are already on the current table and pass through unchanged.
fn normalize_tier(tier: u8, pre_e5a1: bool) -> u8 {
    if pre_e5a1 && tier >= 21 {
        tier + 3
    } else {
        tier
    }
}

// ---------------------------------------------------------------------
// peak / prev_act_tier
// ---------------------------------------------------------------------

/// The highest tier ever won a game at, across every season in
/// `seasons`, normalized per ruling R6. Tiers 0-2 are unranked/unused and
/// ignored even if present with wins; a season not present in
/// `ordered_acts` is skipped (its pre/post-E5A1 status can't be
/// determined). Returns `None` if no tier >= 3 has a recorded win
/// anywhere.
///
/// `ordered_acts` must be sorted by `start_ms` ascending (the shape
/// [`prev_act_tier`] also expects) — this is the "adjust `peak`'s
/// signature minimally to receive the acts" ruling: the brief's original
/// signature took only `seasons`, but normalization needs to know when
/// each season's act started.
pub fn peak(seasons: &SeasonalMap, ordered_acts: &[Act]) -> Option<Peak> {
    let mut best: Option<(u8, &str, i64)> = None; // (tier, season_id, act_start_ms)

    for act in ordered_acts {
        let Some(info) = seasons.get(&act.id) else {
            continue;
        };
        let pre_e5a1 = started_before_e5a1(act, ordered_acts);

        for (&tier, &wins) in &info.wins_by_tier {
            if tier < 3 || wins == 0 {
                continue;
            }
            let normalized = normalize_tier(tier, pre_e5a1);

            let replace = match &best {
                None => true,
                Some((best_tier, _, best_start)) => {
                    normalized > *best_tier
                        || (normalized == *best_tier && act.start_ms >= *best_start)
                }
            };
            if replace {
                best = Some((normalized, act.id.as_str(), act.start_ms));
            }
        }
    }

    best.map(|(tier, season_id, _)| Peak {
        tier,
        season_id: season_id.to_string(),
    })
}

/// The tier reached in the act immediately before the current one.
/// `ordered_acts` is sorted by `start_ms` ascending and already filtered
/// by the caller to acts that have started — its last element is the
/// current act, so the previous act is the one before that. Returns
/// `None` if there is no previous act, it has no stored season, or its
/// tier is unranked (< 3).
pub fn prev_act_tier(seasons: &SeasonalMap, ordered_acts: &[Act]) -> Option<u8> {
    if ordered_acts.len() < 2 {
        return None;
    }
    let prev_act = &ordered_acts[ordered_acts.len() - 2];
    let info = seasons.get(&prev_act.id)?;
    if info.competitive_tier < 3 {
        return None;
    }
    let pre_e5a1 = started_before_e5a1(prev_act, ordered_acts);
    Some(normalize_tier(info.competitive_tier, pre_e5a1))
}

// ---------------------------------------------------------------------
// act_label / act_full_name
// ---------------------------------------------------------------------

/// Parses a Roman numeral act name (`"ACT I"` .. `"ACT VI"` — the only
/// range VALORANT has ever used) into its number.
fn act_number(name: &str) -> Option<u8> {
    match name.trim_start_matches("ACT").trim() {
        "I" => Some(1),
        "II" => Some(2),
        "III" => Some(3),
        "IV" => Some(4),
        "V" => Some(5),
        "VI" => Some(6),
        _ => None,
    }
}

/// The short act label the UI shows next to Peak Rank (e.g. `"E8 A3"` /
/// `"V26 A5"`), derived from real valorant-api.com naming: old-style
/// episode parents are named `"EPISODE {n}"`, new-style ones are named
/// `"V25"`/`"V26"` directly. An act whose name isn't a recognized Roman
/// numeral (a future oddity in the real data) degrades to just the
/// episode part rather than panicking.
pub fn act_label(act: &Act) -> String {
    let episode_part = match act.episode_name.strip_prefix("EPISODE ") {
        Some(n) => format!("E{n}"),
        None => act.episode_name.clone(),
    };
    match act_number(&act.name) {
        Some(n) => format!("{episode_part} A{n}"),
        None => episode_part,
    }
}

/// The full act name used for hover text (e.g. `"V26 ACT V"` /
/// `"EPISODE 8 ACT III"`) — Task 5's `actName` field.
pub fn act_full_name(act: &Act) -> String {
    format!("{} {}", act.episode_name, act.name)
}

// ---------------------------------------------------------------------
// since_act_end
// ---------------------------------------------------------------------

/// Milliseconds since `act` ended, or `None` if it's still the current
/// act (`now_ms < act.end_ms`) — the UI shows "This Act" in that case.
pub fn since_act_end(act: &Act, now_ms: i64) -> Option<i64> {
    if now_ms < act.end_ms {
        None
    } else {
        Some(now_ms - act.end_ms)
    }
}

// ---------------------------------------------------------------------
// split_sessions
// ---------------------------------------------------------------------

/// Splits `starts_ms` (match start times, sorted ascending) into session
/// index ranges: a new session starts whenever the gap to the previous
/// match exceeds `gap_ms` (CLAUDE.md: "A new session starts after a 6h
/// gap with no comp match" — pass [`DEFAULT_SESSION_GAP_MS`] for that).
/// Ranges are contiguous and cover the whole slice; empty input yields no
/// ranges.
pub fn split_sessions(starts_ms: &[i64], gap_ms: i64) -> Vec<Range<usize>> {
    if starts_ms.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    for i in 1..starts_ms.len() {
        if starts_ms[i] - starts_ms[i - 1] > gap_ms {
            ranges.push(start..i);
            start = i;
        }
    }
    ranges.push(start..starts_ms.len());
    ranges
}
