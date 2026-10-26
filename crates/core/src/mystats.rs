//! My Stats aggregation (CLAUDE.md "My Stats"): one pure function over
//! the user's stored rows — no DB, no I/O, no clock, no time-zone
//! database. The caller reads the rows (`Repo::comp_matches` and the
//! user's `comp_updates`, see [`MyStatsInput`]), converts map paths to
//! valorant-api uuids and supplies the local UTC offset.
//!
//! Everything covers the current act only — decided one way for matches
//! and RR rows alike: by Riot's `SeasonID` on the RR row, falling back to
//! valorant-api's act dates only for a match without one (`in_act`) —
//! except "last 7 days", a time window. Each number has one source:
//! - results, maps, agents, K/D, ACS, the session and the weekday /
//!   time-of-day buckets: the act's stored matches;
//! - net RR, the RR chart and rank changes: the act's stored competitive
//!   updates; "last 7 days": the stored updates of the last 7 days;
//! - tier and RR: the stored MMR snapshot (as on the live card); without one,
//!   the live card's fallback chain (R167): the newest current-act RR row, then
//!   a stored current-act match's tier with no RR.

use std::collections::HashMap;

use crate::db::repo::{CompMatchRow, CompUpdateRow, StoredTier};
use crate::model::{
    match_result, match_summary, rank_without_mmr, CurrentRank, Insight, InsightKind,
    MatchResultKind, MyStats, RankChange, RankInfo, RankSummary, RrPoint, SessionMatch,
    SessionSummary, StatRow, StreakMark, WinCell,
};
use crate::stats::{self, Act, DEFAULT_SESSION_GAP_MS};

/// The smallest sample an insight is based on; each side of a comparison
/// needs this many games (the brief: "min 5 games" for best/worst map).
pub const MIN_GAMES: u32 = 5;
/// CLAUDE.md: "Last 5 comp matches".
const LAST5: usize = 5;
/// How many "recent rank changes" the rank card lists.
const RANK_CHANGES: usize = 5;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// The "last 7 days" insight's window — a time window, across act
/// boundaries.
pub const LAST_7_DAYS_MS: i64 = 7 * DAY_MS;
/// Tiers 0–2 are unranked / unused.
const FIRST_RANKED_TIER: u8 = 3;
/// From Immortal 1 up, RR accumulates instead of resetting every 100.
const IMMORTAL_1: u8 = 24;
/// How far past each end of the act's valorant-api dates the caller reads
/// matches: Riot switches acts at each region's patch rollout, hours
/// from valorant-api's midnight-UTC dates, and `in_act` then decides by
/// the RR row's season.
pub const ACT_EDGE_MS: i64 = DAY_MS;
/// Saturday and Sunday (weekday index 0 = Monday).
const WEEKEND: [usize; 2] = [5, 6];
/// In `MyStats::by_daypart` order; lowercase for insight text.
const DAYPARTS: [&str; 4] = ["morning", "afternoon", "evening", "night"];

pub struct MyStatsInput<'a> {
    /// The user's PUUID: sent with the page (`MyStats::puuid`).
    pub puuid: &'a str,
    pub act: &'a Act,
    /// The user's competitive matches, any order — at least those dated
    /// within [`ACT_EDGE_MS`] of the act; rows outside `act` (see
    /// `in_act`) are ignored. `map_id` must already be a valorant-api
    /// uuid (`""` when unknown — such a match is left out of the Maps
    /// table only).
    pub matches: &'a [CompMatchRow],
    /// The user's competitive updates: at least the act's season and the
    /// last [`LAST_7_DAYS_MS`] of any season. Only "last 7 days" reads
    /// other seasons' rows.
    pub comp_updates: &'a [CompUpdateRow],
    /// The current act's rank from the MMR snapshot.
    pub current_rank: Option<CurrentRank>,
    /// R166: the snapshot named this act's tier but left its RR out
    /// (`current_rank` is then `None`): the card shows that tier with the RR
    /// of the newest stored update at it, else none — and so does this page.
    pub mmr_tier_without_rr: Option<u8>,
    /// R167: a stored MMR snapshot of the user answered. It is the authority
    /// for the rank card, even when it has no rank this act (placements);
    /// only without one (`false`) does the card fall back like the live card
    /// (R154): `comp_updates`, then `stored_tier`.
    pub has_mmr: bool,
    /// R167: the user's newest stored competitive match with a ranked tier
    /// (`Repo::newest_ranked_match_tier`), read only without a snapshot; used
    /// only when it is from the current act and no RR row names a rank.
    pub stored_tier: Option<StoredTier>,
    /// valorant-api map uuid -> display name, for insight text. A map
    /// without a name gets no insight rather than a made-up label.
    pub map_names: &'a HashMap<String, String>,
    pub now_ms: i64,
    /// The local UTC offset (ms) at a given UTC instant, so weekday and
    /// time-of-day buckets follow the user's clock (DST included).
    pub utc_offset_ms: &'a dyn Fn(i64) -> i64,
}

/// RR still needed for the next tier. `None` when unranked, and from
/// Immortal 1 up: Immortal RR accumulates with no fixed next-tier step,
/// and Radiant has no next tier.
pub fn rr_to_next(tier: u8, rr: u16) -> Option<u16> {
    (FIRST_RANKED_TIER..IMMORTAL_1)
        .contains(&tier)
        .then(|| 100u16.saturating_sub(rr))
}

/// Builds the whole page. See the module docs for where each number
/// comes from.
pub fn build(input: MyStatsInput) -> MyStats {
    let MyStatsInput {
        puuid,
        act,
        matches,
        comp_updates,
        current_rank,
        mmr_tier_without_rr,
        has_mmr,
        stored_tier,
        map_names,
        now_ms,
        utc_offset_ms,
    } = input;

    let mut played: Vec<Played> = matches
        .iter()
        .filter(|m| in_act(act, m))
        .map(|row| {
            let (weekday, hour) = local_weekday_hour(row.start_ms + utc_offset_ms(row.start_ms));
            Played {
                row,
                result: match_result(&row.team_id, row.winning_team.as_deref()),
                placement: is_placement(row),
                weekday,
                daypart: daypart(hour),
            }
        })
        .collect();
    played.sort_by_key(|p| p.row.start_ms); // oldest first

    let mut updates: Vec<&CompUpdateRow> = comp_updates
        .iter()
        .filter(|u| u.season_id == act.id)
        .collect();
    updates.sort_by_key(|u| u.start_ms); // oldest first

    let mut by_weekday = [Tally::default(); 7];
    let mut by_daypart = [Tally::default(); 4];
    for p in &played {
        by_weekday[p.weekday].add(p);
        by_daypart[p.daypart].add(p);
    }
    let maps = table(&played, |m| {
        Some(m.map_id.as_str()).filter(|id| !id.is_empty())
    });
    let agents = table(&played, |m| m.agent_id.as_deref());

    let mut insights = map_insights(&maps, map_names);
    insights.extend(time_of_day(&by_daypart));
    insights.extend(weekday_vs_weekend(&by_weekday));
    insights.extend(rr_economy(&played));
    insights.extend(last_7_days(comp_updates, now_ms));

    MyStats {
        puuid: puuid.to_string(),
        rank: rank_summary(
            current_rank_of(
                act,
                current_rank,
                mmr_tier_without_rr,
                has_mmr,
                stored_tier,
                comp_updates,
            ),
            &played,
            &updates,
        ),
        rr_series: updates
            .iter()
            .filter_map(|u| {
                let tier = ranked_tier(u.tier_after)?;
                Some(RrPoint {
                    start_ms: u.start_ms,
                    absolute_rr: absolute_rr(tier, u.rr_after) as i32,
                })
            })
            .collect(),
        session: current_session(&played, now_ms),
        last5: played
            .iter()
            .rev()
            .take(LAST5)
            .map(|p| match_summary(p.row))
            .collect(),
        maps,
        agents,
        by_weekday: by_weekday.map(|t| t.cell()),
        by_daypart: by_daypart.map(|t| t.cell()),
        insights,
    }
}

// ---------------------------------------------------------------------
// per-match view + running totals
// ---------------------------------------------------------------------

/// Whether a match counted for `act`: by its RR row's Riot season — the
/// same test the RR rows get — and only without an RR row by valorant-api's
/// act dates. So a game played between valorant-api's act start and
/// Riot's rollout counts for the act Riot counted it for, everywhere.
fn in_act(act: &Act, row: &CompMatchRow) -> bool {
    match &row.season_id {
        Some(season) => *season == act.id,
        None => (act.start_ms..act.end_ms).contains(&row.start_ms),
    }
}

/// A placement game: its RR row has an unranked tier (< 3) on either
/// side — Riot sends tier 0 before and after each placement game until
/// the last one places the player. Its RR change (0) is no ranked win or
/// loss.
fn is_placement(row: &CompMatchRow) -> bool {
    [row.tier_before, row.tier_after]
        .into_iter()
        .flatten()
        .any(|tier| ranked_tier(tier).is_none())
}

/// One of the act's matches with what every section needs precomputed.
struct Played<'a> {
    row: &'a CompMatchRow,
    result: MatchResultKind,
    placement: bool,
    /// Local weekday, 0 = Monday.
    weekday: usize,
    /// Index into [`DAYPARTS`].
    daypart: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    games: u32,
    wins: u32,
    losses: u32,
    /// Kills and deaths only over matches with a stats row (`rounds` known,
    /// R102), counted in `stat_games`.
    kills: i64,
    deaths: i64,
    stat_games: u32,
    /// Score and rounds only over matches with a known round count.
    score: i64,
    rounds: i64,
    /// `None` until a match with a stored RR change is added.
    rr: Option<i32>,
}

impl Tally {
    fn add(&mut self, p: &Played) {
        self.games += 1;
        match p.result {
            MatchResultKind::Win => self.wins += 1,
            MatchResultKind::Loss => self.losses += 1,
            MatchResultKind::Draw => {}
        }
        if let Some(rounds) = p.row.rounds.filter(|&r| r > 0) {
            self.kills += p.row.kills;
            self.deaths += p.row.deaths;
            self.stat_games += 1;
            self.score += p.row.score;
            self.rounds += rounds;
        }
        if let Some(rr) = p.row.rr_earned {
            *self.rr.get_or_insert(0) += rr as i32;
        }
    }

    fn cell(self) -> WinCell {
        WinCell {
            games: self.games,
            wins: self.wins,
            losses: self.losses,
            winrate: stats::winrate(self.wins.into(), self.games.into()),
        }
    }

    /// Only called for a group with at least one game.
    fn row(self, id: String) -> StatRow {
        StatRow {
            id,
            games: self.games,
            wins: self.wins,
            losses: self.losses,
            winrate: self.wins as f32 / self.games as f32,
            net_rr: self.rr,
            kd: (self.stat_games > 0).then(|| stats::kd(self.kills, self.deaths)),
            acs: (self.rounds > 0).then(|| self.score as f32 / self.rounds as f32),
        }
    }
}

/// `(weekday, hour)` of a local-time instant; weekday 0 = Monday.
fn local_weekday_hour(local_ms: i64) -> (usize, i64) {
    // 1970-01-01 was a Thursday (index 3).
    let weekday = (local_ms.div_euclid(DAY_MS) + 3).rem_euclid(7) as usize;
    (weekday, local_ms.rem_euclid(DAY_MS) / HOUR_MS)
}

/// Morning 05–12, Afternoon 12–17, Evening 17–22, Night 22–05.
fn daypart(hour: i64) -> usize {
    match hour {
        5..=11 => 0,
        12..=16 => 1,
        17..=21 => 2,
        _ => 3,
    }
}

/// A stored tier as a ranked tier; `None` for unranked or out of range.
fn ranked_tier(tier: i64) -> Option<u8> {
    u8::try_from(tier).ok().filter(|&t| t >= FIRST_RANKED_TIER)
}

/// See [`RrPoint`]: one line across every rank.
fn absolute_rr(tier: u8, rr: i64) -> i64 {
    i64::from(tier.min(IMMORTAL_1)) * 100 + rr
}

/// The stored RR changes of the ranked (not placement) matches that
/// ended in `kind`.
fn rr_of(played: &[Played], kind: MatchResultKind) -> Vec<i64> {
    played
        .iter()
        .filter(|p| p.result == kind && !p.placement)
        .filter_map(|p| p.row.rr_earned)
        .collect()
}

fn mean(values: &[i64]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<i64>() as f32 / values.len() as f32)
}

/// One Maps/Agents table: grouped by `key` (rows without a key are left
/// out), most played first.
fn table(played: &[Played], key: fn(&CompMatchRow) -> Option<&str>) -> Vec<StatRow> {
    let mut groups: HashMap<&str, Tally> = HashMap::new();
    for p in played {
        if let Some(id) = key(p.row) {
            groups.entry(id).or_default().add(p);
        }
    }
    let mut rows: Vec<StatRow> = groups
        .into_iter()
        .map(|(id, tally)| tally.row(id.to_string()))
        .collect();
    rows.sort_by(|a, b| {
        b.games
            .cmp(&a.games)
            .then(b.wins.cmp(&a.wins))
            .then(a.id.cmp(&b.id))
    });
    rows
}

// ---------------------------------------------------------------------
// rank card / session
// ---------------------------------------------------------------------

/// R167: the rank the card shows — the snapshot's, exact. With no snapshot
/// at all (`has_mmr` false) the live card's chain
/// ([`rank_without_mmr`](crate::model::rank_without_mmr)): the newest
/// current-act RR row (exact RR), else the tier of a stored current-act match
/// (RR unknown). A snapshot with no rank this act stays "no rank".
fn current_rank_of(
    act: &Act,
    snapshot: Option<CurrentRank>,
    tier_without_rr: Option<u8>,
    has_mmr: bool,
    stored_tier: Option<StoredTier>,
    comp_updates: &[CompUpdateRow],
) -> Option<RankInfo> {
    if let Some(c) = snapshot {
        return Some(RankInfo {
            tier: c.tier,
            rr: Some(c.rr),
        });
    }
    if let Some(tier) = tier_without_rr {
        // As the live card (R166): the tier stands, the RR only from an
        // update at that tier.
        let rr = rank_without_mmr(
            Some(comp_updates),
            None,
            None,
            std::slice::from_ref(act),
            &act.id,
        )
        .filter(|r| r.tier == tier)
        .and_then(|r| r.rr);
        return Some(RankInfo { tier, rr });
    }
    if has_mmr {
        return None;
    }
    rank_without_mmr(
        Some(comp_updates),
        None,
        stored_tier,
        std::slice::from_ref(act),
        &act.id,
    )
}

fn rank_summary(
    current: Option<RankInfo>,
    played: &[Played],
    updates: &[&CompUpdateRow],
) -> RankSummary {
    let count = |kind| played.iter().filter(|p| p.result == kind).count() as u32;
    let any_match = !played.is_empty();
    RankSummary {
        tier: current.map(|c| c.tier),
        rr: current.and_then(|c| c.rr),
        wins: any_match.then(|| count(MatchResultKind::Win)),
        losses: any_match.then(|| count(MatchResultKind::Loss)),
        rr_to_next: current.and_then(|c| rr_to_next(c.tier, c.rr?)),
        net_rr: (!updates.is_empty()).then(|| updates.iter().map(|u| u.rr_earned as i32).sum()),
        avg_win: mean(&rr_of(played, MatchResultKind::Win)),
        avg_loss: mean(&rr_of(played, MatchResultKind::Loss)),
        // Placements (unranked -> ranked) aren't a rank change.
        changes: updates
            .iter()
            .rev()
            .filter_map(|u| {
                let before = ranked_tier(u.tier_before?)?;
                let after = ranked_tier(u.tier_after)?;
                (before != after).then_some(RankChange {
                    tier: after,
                    up: after > before,
                    start_ms: u.start_ms,
                })
            })
            .take(RANK_CHANGES)
            .collect(),
    }
}

/// The session still going at `now_ms`: the one holding the newest match
/// (CLAUDE.md: "A new session starts after a 6h gap with no comp match"),
/// unless that match started more than 6h before `now_ms` — then the next
/// match will start a new session, and there's none now (`None`). Gaps
/// are measured start to start, as between matches. `played` is oldest
/// first.
fn current_session(played: &[Played], now_ms: i64) -> Option<SessionSummary> {
    let newest = played.last()?.row.start_ms;
    if now_ms - newest > DEFAULT_SESSION_GAP_MS {
        return None;
    }
    let starts: Vec<i64> = played.iter().map(|p| p.row.start_ms).collect();
    let games = &played[stats::split_sessions(&starts, DEFAULT_SESSION_GAP_MS).pop()?];
    let mut tally = Tally::default();
    for p in games {
        tally.add(p);
    }
    Some(SessionSummary {
        net_rr: tally.rr,
        wins: tally.wins,
        losses: tally.losses,
        started_ms: games[0].row.start_ms,
        matches: games.iter().map(session_match).collect(),
    })
}

/// One chip of the session: the result, plus the map, the RR change and
/// the start the chip's tooltip shows and the match id its click opens.
/// `row.map_id` is already a valorant-api uuid (`""` when unknown, sent as
/// `None`).
fn session_match(p: &Played) -> SessionMatch {
    SessionMatch {
        match_id: p.row.match_id.clone(),
        map_id: (!p.row.map_id.is_empty()).then(|| p.row.map_id.clone()),
        rr_earned: p.row.rr_earned.map(|rr| rr as i32),
        start_ms: p.row.start_ms,
        result: match p.result {
            MatchResultKind::Win => StreakMark::Win,
            MatchResultKind::Loss => StreakMark::Loss,
            MatchResultKind::Draw => StreakMark::Draw,
        },
    }
}

// ---------------------------------------------------------------------
// insights
// ---------------------------------------------------------------------

/// A labelled win record — a map, a time of day, weekdays or weekends.
#[derive(Clone, Copy)]
struct Bucket<'a> {
    label: &'a str,
    games: u32,
    wins: u32,
}

impl Bucket<'_> {
    /// Rounded win percentage, as the insight text shows it.
    fn pct(&self) -> i64 {
        (100.0 * f64::from(self.wins) / f64::from(self.games)).round() as i64
    }
}

/// The best and the worst of the buckets with [`MIN_GAMES`]+ games —
/// `None` unless there are two whose shown percentages differ.
fn best_and_worst<'a>(
    buckets: impl IntoIterator<Item = Bucket<'a>>,
) -> Option<(Bucket<'a>, Bucket<'a>)> {
    let mut ranked: Vec<Bucket> = buckets
        .into_iter()
        .filter(|b| b.games >= MIN_GAMES)
        .collect();
    // Exact win rate, highest first (ties: more games first).
    ranked.sort_by(|a, b| {
        (u64::from(b.wins) * u64::from(a.games))
            .cmp(&(u64::from(a.wins) * u64::from(b.games)))
            .then(b.games.cmp(&a.games))
    });
    let (best, worst) = (*ranked.first()?, *ranked.last()?);
    (best.pct() > worst.pct()).then_some((best, worst))
}

fn compare_text(best: Bucket, worst: Bucket) -> String {
    format!(
        "You win {}% of {} {} games, compared with {}% of {} {} games.",
        best.pct(),
        best.games,
        best.label,
        worst.pct(),
        worst.games,
        worst.label
    )
}

fn map_insights(maps: &[StatRow], map_names: &HashMap<String, String>) -> Vec<Insight> {
    let buckets = maps.iter().map(|r| Bucket {
        label: &r.id,
        games: r.games,
        wins: r.wins,
    });
    let Some((best, worst)) = best_and_worst(buckets) else {
        return Vec::new();
    };
    [
        (best, InsightKind::BestMap, "best", true),
        (worst, InsightKind::WorstMap, "weakest", false),
    ]
    .into_iter()
    .filter_map(|(bucket, kind, word, positive)| {
        let name = map_names.get(bucket.label)?;
        Some(Insight {
            kind,
            text: format!(
                "{name} is your {word} map: {}% wins in {} games.",
                bucket.pct(),
                bucket.games
            ),
            matches: bucket.games,
            positive,
        })
    })
    .collect()
}

fn time_of_day(by_daypart: &[Tally; 4]) -> Option<Insight> {
    let buckets = by_daypart.iter().zip(DAYPARTS).map(|(t, label)| Bucket {
        label,
        games: t.games,
        wins: t.wins,
    });
    let (best, worst) = best_and_worst(buckets)?;
    Some(Insight {
        kind: InsightKind::TimeOfDay,
        text: compare_text(best, worst),
        matches: best.games + worst.games,
        positive: true,
    })
}

fn weekday_vs_weekend(by_weekday: &[Tally; 7]) -> Option<Insight> {
    let mut weekend = Bucket {
        label: "weekend",
        games: 0,
        wins: 0,
    };
    let mut weekday = Bucket {
        label: "weekday",
        ..weekend
    };
    for (day, t) in by_weekday.iter().enumerate() {
        let bucket = if WEEKEND.contains(&day) {
            &mut weekend
        } else {
            &mut weekday
        };
        bucket.games += t.games;
        bucket.wins += t.wins;
    }
    let (best, worst) = best_and_worst([weekend, weekday])?;
    Some(Insight {
        kind: InsightKind::WeekdayWeekend,
        text: compare_text(best, worst),
        matches: best.games + worst.games,
        positive: true,
    })
}

/// Average RR per ranked win and loss — [`MIN_GAMES`]+ on each side.
fn rr_economy(played: &[Played]) -> Option<Insight> {
    let wins = rr_of(played, MatchResultKind::Win);
    let losses = rr_of(played, MatchResultKind::Loss);
    if wins.len().min(losses.len()) < MIN_GAMES as usize {
        return None;
    }
    let matches = (wins.len() + losses.len()) as u32;
    // Rounded as the text shows them, and judged on those.
    let avg_win = mean(&wins)?.round() as i32;
    let avg_loss = mean(&losses)?.round() as i32;
    Some(Insight {
        kind: InsightKind::RrEconomy,
        text: format!("Average {avg_win:+} RR on wins and {avg_loss:+} RR on losses."),
        matches,
        positive: avg_win + avg_loss >= 0,
    })
}

/// Every stored RR change of the last 7 days, whichever act it was in.
fn last_7_days(updates: &[CompUpdateRow], now_ms: i64) -> Option<Insight> {
    let recent: Vec<i64> = updates
        .iter()
        .filter(|u| u.start_ms >= now_ms - LAST_7_DAYS_MS)
        .map(|u| u.rr_earned)
        .collect();
    let matches = recent.len() as u32;
    if matches < MIN_GAMES {
        return None;
    }
    let net: i64 = recent.iter().sum();
    Some(Insight {
        kind: InsightKind::Last7Days,
        text: format!("{net:+} RR over {matches} games in the last 7 days."),
        matches,
        positive: net >= 0,
    })
}
