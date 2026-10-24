//! The Match Details scoreboard (Task 15, spec §6.10): a stored match, seen
//! by the user (or, for a match the user didn't play, from the focus
//! player's side), as the UI receives it. Pure — no DB, no I/O: the shell
//! reads the [`StoredMatch`] and the name sources, this decides what each
//! row shows.
//!
//! # Names (R74, R93, R145)
//!
//! For every row, in this order:
//! 1. the user → their own name (from the local chat session, else the
//!    stored or `players`-table name), shown as is;
//! 2. a puuid that is hidden on the live screen now (`hidden_now`) → what its
//!    card shows: the remembered name (the DB knows it from any source,
//!    R145), else the row's agent, else "Hidden Player" — in every view,
//!    whether or not the user played the match;
//! 3. otherwise a stored name is shown as match-details gave it (match-details
//!    carries no incognito flag, so this is R74's known risk for matches the
//!    user did not play);
//! 4. a row without a stored name shows the `players`-table name (`seen`, a
//!    name-service answer from a time the player was visible) as a plain
//!    name (R145);
//! 5. with neither, the rule-2 result.
//!
//! The `remembered` sources come from `Repo::known_name` (any stored source)
//! and the `seen` names from `Repo::seen_name`; a remembered name's Riot map
//! path becomes a valorant-api uuid here, exactly once, via
//! [`KnownName::with_map_uuid`] (R84, R95). Nothing here fetches a name.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use crate::db::repo::{MatchPlayer, MatchTeam, StoredMatch};
use crate::model::{
    display_tier, match_result, DisplayName, MatchDetailsView, MatchTeamView, Perspective, ScoreRow,
};
use crate::names::{display_name, KnownName};
use crate::stats;

/// Everything [`build_match_view`] needs besides the match itself.
pub struct MatchViewInput<'a> {
    pub stored: &'a StoredMatch,
    /// Riot map path -> valorant-api uuid: the match's own map, and every
    /// remembered name's source map (R84, R95).
    pub map_uuid: &'a dyn Fn(&str) -> Option<String>,
    pub self_puuid: Option<&'a str>,
    /// The user's own Riot ID (name, tag) from the chat session, when known.
    pub self_name: Option<(String, String)>,
    /// The player whose Details the match was opened from.
    pub focus_puuid: &'a str,
    /// `LiveModel::hidden_players()`: puuid -> the agent their card shows.
    pub hidden_now: &'a HashMap<String, Option<String>>,
    /// `Repo::known_name(self, p)` for the rows that need one
    /// ([`needs_known_name`]): the best known name from any source (R145).
    /// `map_id` is still a Riot path here.
    pub remembered: &'a HashMap<String, KnownName>,
    /// `Repo::seen_name(p)` for the rows that need one ([`needs_seen_name`]):
    /// the name a name-service answer stored while the player was visible.
    pub seen: &'a HashMap<String, (String, String)>,
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.is_empty())
}

/// `(name, tag)` as stored, when both halves are present.
fn stored_name(p: &MatchPlayer) -> Option<(String, String)> {
    Some((
        non_empty(&p.game_name)?.to_string(),
        non_empty(&p.tag_line)?.to_string(),
    ))
}

/// Whether a row has no full `(name, tag)` stored in its match (R74:
/// match-details blanks the names of players who hid them). Unlike
/// [`needs_seen_name`] it ignores who is hidden on the live screen: the names
/// fill asks about such a row whoever it is (R153).
pub fn lacks_stored_name(p: &MatchPlayer) -> bool {
    stored_name(p).is_none()
}

/// Whether a row other than the user's is not shown by its stored name:
/// hidden on the live screen now (R93), or without a stored name (R74). The
/// shell looks up a remembered name (`Repo::known_name`) for exactly these
/// rows. A row without a stored name may still be drawn as a plain name from
/// the `players` table ([`needs_seen_name`]); it is drawn in the hidden style
/// only when that is missing too.
pub fn needs_known_name(p: &MatchPlayer, hidden_now: &HashMap<String, Option<String>>) -> bool {
    hidden_now.contains_key(&p.puuid) || stored_name(p).is_none()
}

/// Whether a row's `players`-table name (`Repo::seen_name`) can be drawn: it
/// has no stored name and is not hidden on the live screen now (R145). A row
/// hidden now keeps its card's rule, so its seen name is never a plain one.
pub fn needs_seen_name(p: &MatchPlayer, hidden_now: &HashMap<String, Option<String>>) -> bool {
    !hidden_now.contains_key(&p.puuid) && stored_name(p).is_none()
}

/// The name for a row that isn't shown by a plain name: what a hidden
/// player's card shows (remembered, else the agent, else Hidden Player).
fn hidden_style(input: &MatchViewInput, p: &MatchPlayer) -> DisplayName {
    display_name(
        true,
        None,
        non_empty(&p.agent_id),
        input
            .remembered
            .get(&p.puuid)
            .cloned()
            .map(|known| known.with_map_uuid(input.map_uuid)),
    )
}

fn row_name(input: &MatchViewInput, p: &MatchPlayer) -> DisplayName {
    if input.self_puuid == Some(p.puuid.as_str()) {
        // The user's own name: the chat session's, else the stored one, else
        // the `players`-table one (rule 4: with VALORANT closed there is no
        // chat session, and match-details may have blanked the row).
        if let Some(name) = input
            .self_name
            .clone()
            .or_else(|| stored_name(p))
            .or_else(|| input.seen.get(&p.puuid).cloned())
        {
            return display_name(false, Some(name), None, None);
        }
        return hidden_style(input, p);
    }
    if input.hidden_now.contains_key(&p.puuid) {
        return hidden_style(input, p);
    }
    match stored_name(p).or_else(|| input.seen.get(&p.puuid).cloned()) {
        Some(name) => display_name(false, Some(name), None, None),
        None => hidden_style(input, p),
    }
}

fn score_row(input: &MatchViewInput, p: &MatchPlayer) -> ScoreRow {
    // Match-details stores no round count when the player had no stats row:
    // then K/D/A, ACS, ADR and HS% are unknown, not zero (R124).
    let rounds = p.rounds.filter(|&r| r > 0);
    ScoreRow {
        puuid: p.puuid.clone(),
        name: row_name(input, p),
        agent_id: non_empty(&p.agent_id).map(str::to_string),
        tier: display_tier(p.competitive_tier),
        kills: rounds.map(|_| p.kills),
        deaths: rounds.map(|_| p.deaths),
        assists: rounds.map(|_| p.assists),
        acs: rounds.map(|r| p.score as f32 / r as f32),
        adr: rounds.zip(p.damage).map(|(r, d)| d as f32 / r as f32),
        hs_pct: stats::hs_pct(p.hs, p.bs, p.ls).filter(|_| rounds.is_some()),
        is_self: input.self_puuid == Some(p.puuid.as_str()),
        is_focus: p.puuid == input.focus_puuid,
    }
}

/// ACS descending, rows without an ACS last, ties by puuid.
fn by_acs(a: &ScoreRow, b: &ScoreRow) -> Ordering {
    match (a.acs, b.acs) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| a.puuid.cmp(&b.puuid))
}

fn sorted(mut rows: Vec<ScoreRow>) -> Vec<ScoreRow> {
    rows.sort_by(by_acs);
    rows
}

/// One team's block: result and score from its `match_teams` row.
fn team_view(stored: &StoredMatch, team_id: &str, rows: Vec<ScoreRow>) -> MatchTeamView {
    let row: Option<&MatchTeam> = stored.teams.iter().find(|t| t.team_id == team_id);
    MatchTeamView {
        team_id: team_id.to_string(),
        // Without any team rows the result is unknown; with them, a match
        // without a winning team is a draw for everyone.
        result: (!stored.teams.is_empty())
            .then(|| match_result(team_id, stored.info.winning_team.as_deref())),
        rounds_won: row.and_then(|t| t.rounds_won),
        first_half: row.and_then(|t| t.rounds_won_h1),
        players: sorted(rows),
    }
}

/// Builds the Match Details view (see the module docs for the name rules).
///
/// `teams[0]` is the user's team when the user has a row in the match
/// ([`Perspective::SelfView`]), else the focus player's. Anything other
/// than exactly two teams (Deathmatch: every player is their own team) is a
/// single block with an empty `team_id` and no result, score or halves.
pub fn build_match_view(input: MatchViewInput) -> MatchDetailsView {
    let stored = input.stored;
    let rows: Vec<(String, ScoreRow)> = stored
        .players
        .iter()
        .map(|p| (p.team_id.clone(), score_row(&input, p)))
        .collect();

    let user_team = input
        .self_puuid
        .and_then(|me| stored.players.iter().find(|p| p.puuid == me))
        .map(|p| p.team_id.as_str());
    let perspective = if user_team.is_some() {
        Perspective::SelfView
    } else {
        Perspective::Focus
    };
    let lead_team = user_team.or_else(|| {
        stored
            .players
            .iter()
            .find(|p| p.puuid == input.focus_puuid)
            .map(|p| p.team_id.as_str())
    });

    let team_ids: BTreeSet<&str> = stored.players.iter().map(|p| p.team_id.as_str()).collect();
    let teams = if team_ids.len() == 2 {
        // The perspective team first, the other after it.
        let mut ordered: Vec<&str> = team_ids.iter().copied().collect();
        if let Some(lead) = lead_team {
            ordered.sort_by_key(|id| *id != lead);
        }
        ordered
            .into_iter()
            .map(|id| {
                let team_rows = rows
                    .iter()
                    .filter(|(team, _)| team == id)
                    .map(|(_, row)| row.clone())
                    .collect();
                team_view(stored, id, team_rows)
            })
            .collect()
    } else {
        vec![MatchTeamView {
            team_id: String::new(),
            result: None,
            rounds_won: None,
            first_half: None,
            players: sorted(rows.into_iter().map(|(_, row)| row).collect()),
        }]
    };

    MatchDetailsView {
        match_id: stored.info.match_id.clone(),
        map_id: (input.map_uuid)(&stored.info.map_id).unwrap_or_default(),
        queue_id: stored.info.queue_id.clone(),
        start_ms: stored.info.start_ms,
        duration_ms: stored.info.game_length_ms,
        perspective,
        teams,
    }
}
