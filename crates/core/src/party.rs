//! CLAUDE.md "Party detection": which players on screen queued together.
//!
//! Evidence is each player's last two matches in **any** mode (Deathmatch
//! included), from the DB — no extra requests:
//! - two players whose **most recent** match is the same match, on the
//!   same team, are a **likely** party (Deathmatch has no teams, so the
//!   same match is enough there);
//! - that link is **confirmed** when their last two matches are both
//!   shared on the same team, **or** when the shared most recent match
//!   gives both the same non-empty `partyId` (one game is then enough);
//! - links are transitive (A–B + B–C = one group A–B–C), and a group is
//!   confirmed only if every link inside it is.
//!
//! The user and their own party are excluded entirely (their party comes
//! straight from the client; they get no stripe) — including as a bridge,
//! so nobody is linked to someone else through the user.

use std::collections::{HashMap, HashSet};

use crate::model::{PartyMark, Team};

/// One of a player's recent matches, as far as party detection cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedRef {
    pub match_id: String,
    /// The player's team in that match; `None` for a mode without teams
    /// (Deathmatch).
    pub team_id: Option<String>,
    /// Riot's `partyId` from match-details, when stored.
    pub party_id: Option<String>,
}

/// Same match, and same team (or both without a team).
fn together(a: &SharedRef, b: &SharedRef) -> bool {
    a.match_id == b.match_id && a.team_id == b.team_id
}

fn same_party(a: &SharedRef, b: &SharedRef) -> bool {
    matches!((&a.party_id, &b.party_id), (Some(x), Some(y)) if !x.is_empty() && x == y)
}

/// Union-find root with path halving.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Party marks for the players on screen.
///
/// - `players`: `(puuid, live team)` in roster (display) order. Players on
///   opposite live teams are never linked — they can't be queued together
///   right now, whatever their history says.
/// - `recent`: each player's last two matches, newest first; a missing
///   entry, or `None` in a slot, is "unknown" and links nothing.
/// - `self_party`: the user and their party (never marked, never a link).
///
/// Only players in a group get a mark. Group colors are `0..n`, numbered
/// by each group's first player in roster order.
pub fn detect_parties(
    players: &[(String, Team)],
    recent: &HashMap<String, [Option<SharedRef>; 2]>,
    self_party: &HashSet<String>,
) -> HashMap<String, PartyMark> {
    let candidates: Vec<(&str, Team, &[Option<SharedRef>; 2])> = players
        .iter()
        .filter(|(puuid, _)| !self_party.contains(puuid))
        .filter_map(|(puuid, team)| recent.get(puuid).map(|r| (puuid.as_str(), *team, r)))
        .collect();

    let n = candidates.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut links: Vec<(usize, bool)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            let (_, team_a, [a0, a1]) = candidates[i];
            let (_, team_b, [b0, b1]) = candidates[j];
            if team_a != team_b {
                continue;
            }
            let (Some(a0), Some(b0)) = (a0, b0) else {
                continue;
            };
            if !together(a0, b0) {
                continue;
            }
            let both_shared = matches!((a1, b1), (Some(a1), Some(b1)) if together(a1, b1));
            let confirmed = both_shared || same_party(a0, b0);
            let (ra, rb) = (find(&mut parent, i), find(&mut parent, j));
            parent[ra] = rb;
            links.push((i, confirmed));
        }
    }

    // A group is confirmed only if all of its links are.
    let mut group_confirmed: HashMap<usize, bool> = HashMap::new();
    for (i, confirmed) in links {
        let root = find(&mut parent, i);
        *group_confirmed.entry(root).or_insert(true) &= confirmed;
    }

    let mut colors: HashMap<usize, u8> = HashMap::new();
    let mut marks = HashMap::new();
    for (i, (puuid, _, _)) in candidates.iter().enumerate() {
        let root = find(&mut parent, i);
        let Some(&confirmed) = group_confirmed.get(&root) else {
            continue; // linked to nobody
        };
        let next = u8::try_from(colors.len()).unwrap_or(u8::MAX);
        let group = *colors.entry(root).or_insert(next);
        marks.insert(puuid.to_string(), PartyMark { group, confirmed });
    }
    marks
}
