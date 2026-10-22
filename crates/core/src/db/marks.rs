//! R189 (user 2026-10-22): the v2 redesign's own stored data, migration 4 —
//! saved players, private notes, dismissed smurf flags ([`PlayerMarks`]),
//! the Saved screen's rows ([`Repo::saved_rows`] + [`saved_player`]) and the
//! ASCII studio's gallery ([`AsciiArt`]).
//!
//! The Tauri commands in `src-tauri/src/v2/marks.rs` and `v2/ascii.rs` are
//! thin wrappers: the SQL and the rules (caps, the empty note, the hidden-name
//! rules of a saved player) live here, in `opd1-core`, so the tests and the
//! contract goldens run the real code without a Tauri app.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension, Result};

use super::repo::Repo;
use crate::model::{display_tier, DisplayName};
use crate::names::{display_name, KnownName};
use crate::wire::{AsciiArt, PlayerMarks, SavedPlayer, SharedRecord};

/// A note longer than this many characters is cut to it (R189): a private
/// note is a line or two, and the whole map travels in every `"marks"` event.
pub const NOTE_MAX_CHARS: usize = 2000;
/// An ASCII piece larger than this many bytes (UTF-8) is refused: art cut
/// mid-row is broken art, so the studio is told instead of storing half.
pub const ASCII_RAW_MAX_BYTES: usize = 8 * 1024;
/// An ASCII piece's title is trimmed and cut to this many characters.
pub const ASCII_TITLE_MAX_CHARS: usize = 40;

/// The first `max` characters of `s` (never splits a character).
fn cut_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((at, _)) => &s[..at],
        None => s,
    }
}

/// What [`Repo::set_note`] stores for `note`: `None` (the row is deleted)
/// for an empty or whitespace-only note, else the note cut to
/// [`NOTE_MAX_CHARS`]. The text is otherwise kept as typed (line breaks
/// included).
pub fn normalize_note(note: &str) -> Option<String> {
    (!note.trim().is_empty()).then(|| cut_chars(note, NOTE_MAX_CHARS).to_string())
}

/// Why [`check_ascii`] refused a piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsciiRefusal {
    Empty,
    TooLarge,
}

impl std::fmt::Display for AsciiRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsciiRefusal::Empty => write!(f, "the art is empty"),
            AsciiRefusal::TooLarge => {
                write!(f, "the art is larger than {ASCII_RAW_MAX_BYTES} bytes")
            }
        }
    }
}

/// The `(title, raw)` [`Repo::ascii_save`] stores: the title trimmed and cut
/// to [`ASCII_TITLE_MAX_CHARS`]; the art as given, refused when it is blank
/// or over [`ASCII_RAW_MAX_BYTES`].
pub fn check_ascii(title: &str, raw: &str) -> std::result::Result<(String, String), AsciiRefusal> {
    if raw.trim().is_empty() {
        return Err(AsciiRefusal::Empty);
    }
    if raw.len() > ASCII_RAW_MAX_BYTES {
        return Err(AsciiRefusal::TooLarge);
    }
    Ok((
        cut_chars(title.trim(), ASCII_TITLE_MAX_CHARS).to_string(),
        raw.to_string(),
    ))
}

// ---------------------------------------------------------------------
// marks
// ---------------------------------------------------------------------

impl Repo {
    /// Every mark: saved puuids newest saved first (ties by puuid, so the
    /// order is stable), the non-empty notes, the dismissed smurf flags
    /// (newest dismissed first). Three small table reads.
    pub fn player_marks(&self) -> Result<PlayerMarks> {
        let conn = self.conn();
        let saved = conn
            .prepare("SELECT puuid FROM saved_players ORDER BY saved_ms DESC, puuid")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>>>()?;
        let notes = conn
            .prepare("SELECT puuid, note FROM player_notes")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<BTreeMap<String, String>>>()?;
        let not_smurf = conn
            .prepare("SELECT puuid FROM smurf_dismissed ORDER BY dismissed_ms DESC, puuid")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>>>()?;
        Ok(PlayerMarks {
            saved,
            notes,
            not_smurf,
        })
    }

    /// Whether `puuid` is saved: one primary-key lookup.
    pub fn is_saved(&self, puuid: &str) -> Result<bool> {
        self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM saved_players WHERE puuid = ?1)",
            params![puuid],
            |r| r.get(0),
        )
    }

    /// The private note on `puuid`, `None` when there is none.
    pub fn note(&self, puuid: &str) -> Result<Option<String>> {
        self.conn()
            .query_row(
                "SELECT note FROM player_notes WHERE puuid = ?1",
                params![puuid],
                |r| r.get(0),
            )
            .optional()
    }

    /// Saves (`saved`) or unsaves `puuid`. Saving an already saved player
    /// keeps the first `saved_ms`, so the list order does not jump.
    pub fn set_saved(&self, puuid: &str, saved: bool, now_ms: i64) -> Result<()> {
        if saved {
            self.conn().execute(
                "INSERT INTO saved_players (puuid, saved_ms) VALUES (?1, ?2)
                 ON CONFLICT(puuid) DO NOTHING",
                params![puuid, now_ms],
            )?;
        } else {
            self.conn()
                .execute("DELETE FROM saved_players WHERE puuid = ?1", params![puuid])?;
        }
        Ok(())
    }

    /// Stores the note on `puuid` as [`normalize_note`] says: an empty or
    /// whitespace-only note deletes the row, a long one is cut.
    pub fn set_note(&self, puuid: &str, note: &str, now_ms: i64) -> Result<()> {
        match normalize_note(note) {
            Some(note) => self.conn().execute(
                "INSERT INTO player_notes (puuid, note, updated_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(puuid) DO UPDATE SET
                    note = excluded.note,
                    updated_ms = excluded.updated_ms",
                params![puuid, note, now_ms],
            )?,
            None => self
                .conn()
                .execute("DELETE FROM player_notes WHERE puuid = ?1", params![puuid])?,
        };
        Ok(())
    }

    /// "Not a smurf": `puuid`'s smurf flag stays hidden. Dismissing twice
    /// keeps the first time.
    pub fn dismiss_smurf(&self, puuid: &str, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO smurf_dismissed (puuid, dismissed_ms) VALUES (?1, ?2)
             ON CONFLICT(puuid) DO NOTHING",
            params![puuid, now_ms],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------
// the Saved screen
// ---------------------------------------------------------------------

/// One saved player as the DB knows them, before the hidden-name rules and
/// the map conversion ([`saved_player`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedRow {
    pub puuid: String,
    pub saved_ms: i64,
    /// `""` when there is none.
    pub note: String,
    /// The best name the DB knows from any source ([`Repo::known_name`],
    /// R145); its `map_id` is still the stored Riot path.
    pub known_name: Option<KnownName>,
    /// The agent of their newest stored match (any queue) that has one.
    pub agent_id: Option<String>,
    /// The newest known competitive tier (see [`Repo::saved_rows`]).
    pub tier: Option<u8>,
    pub with: SharedRecord,
    pub against: SharedRecord,
    /// The newest stored match (any queue) the user shared with them: its
    /// start and its stored Riot map path.
    pub last_seen_ms: Option<i64>,
    pub last_seen_map: Option<String>,
}

/// The agent of `?1`'s newest stored match (any queue) that names one: an
/// index seek on `idx_match_players_puuid` and a primary-key read of
/// `matches` per stored match of the player.
const NEWEST_AGENT_SQL: &str = "\
SELECT mp.agent_id
FROM match_players mp
JOIN matches m ON m.match_id = mp.match_id
WHERE mp.puuid = ?1 AND mp.agent_id IS NOT NULL AND mp.agent_id != ''
ORDER BY m.start_ms DESC
LIMIT 1";

/// `?1`'s newest RR row (`idx_comp_updates_puuid_start`).
const NEWEST_COMP_UPDATE_SQL: &str = "\
SELECT start_ms, tier_after FROM comp_updates
WHERE puuid = ?1
ORDER BY start_ms DESC
LIMIT 1";

/// The newest stored match of any queue that both the user (`?1`) and `?2`
/// played. Driven from `?2`'s rows like `ENCOUNTERS_SQL` (the user is in
/// thousands of matches, a saved player in few); `CROSS JOIN` pins that order.
const LAST_SHARED_SQL: &str = "\
SELECT m.start_ms, m.map_id
FROM match_players mp_other
CROSS JOIN match_players mp_self
  ON mp_self.match_id = mp_other.match_id AND mp_self.puuid = ?1
JOIN matches m ON m.match_id = mp_other.match_id
WHERE mp_other.puuid = ?2
ORDER BY m.start_ms DESC
LIMIT 1";

impl Repo {
    /// The Saved screen's rows, newest saved first, from the DB only.
    /// `self_puuid` is the user (`None` before the app ever knew them: no
    /// shared record, no last-seen).
    ///
    /// Per saved player, a handful of indexed reads (the list is short):
    /// - the name the DB knows from any source ([`Repo::known_name`]);
    /// - the agent of their newest stored match that names one;
    /// - the tier: the newer of their newest RR row's `tier_after` and their
    ///   newest ranked competitive match's tier
    ///   ([`Repo::newest_ranked_match_tier`]), the RR row on a tie; a tier
    ///   below 3 (unranked, placements) counts as not known;
    /// - with / against: their stored COMPETITIVE matches with the user
    ///   ([`Repo::encounters`], R156), on the same team / the other one,
    ///   wins and losses from the user's side (a draw counts as a game only);
    /// - last seen: the newest stored match of ANY queue shared with the
    ///   user (an unrated or Deathmatch together is still "seen").
    ///
    /// The user saving themselves gets no shared record and no last-seen
    /// (every match would be "with").
    pub fn saved_rows(&self, self_puuid: Option<&str>) -> Result<Vec<SavedRow>> {
        let saved: Vec<(String, i64)> = self
            .conn()
            .prepare("SELECT puuid, saved_ms FROM saved_players ORDER BY saved_ms DESC, puuid")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_>>()?;
        let mut agent_stmt = self.conn().prepare(NEWEST_AGENT_SQL)?;
        let mut rr_stmt = self.conn().prepare(NEWEST_COMP_UPDATE_SQL)?;
        let mut seen_stmt = self.conn().prepare(LAST_SHARED_SQL)?;

        let mut out = Vec::with_capacity(saved.len());
        for (puuid, saved_ms) in saved {
            let agent_id: Option<String> = agent_stmt
                .query_row(params![puuid], |r| r.get(0))
                .optional()?;

            let rr_tier: Option<(i64, i64)> = rr_stmt
                .query_row(params![puuid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let rr_tier =
                rr_tier.and_then(|(start_ms, tier)| Some((start_ms, display_tier(Some(tier))?)));
            let match_tier = self
                .newest_ranked_match_tier(&puuid)?
                .map(|t| (t.start_ms, t.tier));
            let tier = match (rr_tier, match_tier) {
                (Some(rr), Some(m)) => Some(if m.0 > rr.0 { m.1 } else { rr.1 }),
                (rr, m) => rr.or(m).map(|(_, t)| t),
            };

            let mut with = SharedRecord::default();
            let mut against = SharedRecord::default();
            let (mut last_seen_ms, mut last_seen_map) = (None, None);
            let me = self_puuid.filter(|me| *me != puuid);
            if let Some(me) = me {
                for e in self.encounters(me, &puuid)?.matches {
                    let side = if e.with { &mut with } else { &mut against };
                    side.games += 1;
                    match e.self_won {
                        Some(true) => side.wins += 1,
                        Some(false) => side.losses += 1,
                        None => {}
                    }
                }
                if let Some((ms, map)) = seen_stmt
                    .query_row(params![me, puuid], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                    })
                    .optional()?
                {
                    last_seen_ms = Some(ms);
                    last_seen_map = Some(map);
                }
            }

            out.push(SavedRow {
                known_name: self.known_name(self_puuid.unwrap_or(""), &puuid)?,
                note: self.note(&puuid)?.unwrap_or_default(),
                puuid,
                saved_ms,
                agent_id,
                tier,
                with,
                against,
                last_seen_ms,
                last_seen_map,
            });
        }
        Ok(out)
    }
}

/// One Saved-screen row on the wire. `live_hidden` is `Some(agent)` when the
/// player is incognito in the current live match (the agent their card shows
/// once locked, R99): then the live rules apply (R18/R145) — the name the DB
/// knows in the remembered style, else the agent, else Hidden Player. Off the
/// live screen a known name is shown plainly (R145), and a player whose name
/// the DB never learned is Hidden Player. `map_uuid` turns stored Riot map
/// paths into valorant-api uuids; an unknown map is `None`, never a path.
pub fn saved_player(
    row: SavedRow,
    live_hidden: Option<Option<&str>>,
    map_uuid: &dyn Fn(&str) -> Option<String>,
) -> SavedPlayer {
    let name: DisplayName = match live_hidden {
        Some(agent) => display_name(
            true,
            None,
            agent,
            row.known_name.map(|k| k.with_map_uuid(map_uuid)),
        ),
        None => display_name(false, row.known_name.map(|k| (k.name, k.tag)), None, None),
    };
    SavedPlayer {
        puuid: row.puuid,
        name,
        agent_id: row.agent_id,
        tier: row.tier,
        with: row.with,
        against: row.against,
        note: row.note,
        saved_ms: row.saved_ms,
        last_seen_ms: row.last_seen_ms,
        last_seen_map_id: row
            .last_seen_map
            .as_deref()
            .and_then(map_uuid)
            .filter(|m| !m.starts_with('/')),
    }
}

// ---------------------------------------------------------------------
// ASCII gallery
// ---------------------------------------------------------------------

fn ascii_row(r: &rusqlite::Row) -> Result<AsciiArt> {
    Ok(AsciiArt {
        id: r.get(0)?,
        title: r.get(1)?,
        raw: r.get(2)?,
        created_ms: r.get(3)?,
    })
}

impl Repo {
    /// The gallery, newest first (ties: the later id first).
    pub fn ascii_list(&self) -> Result<Vec<AsciiArt>> {
        self.conn()
            .prepare(
                "SELECT id, title, raw, created_ms FROM ascii_art
                 ORDER BY created_ms DESC, id DESC",
            )?
            .query_map([], ascii_row)?
            .collect()
    }

    /// Stores a piece as given (the caller ran [`check_ascii`]) and returns
    /// it with its new id.
    pub fn ascii_save(&self, title: &str, raw: &str, now_ms: i64) -> Result<AsciiArt> {
        self.conn().execute(
            "INSERT INTO ascii_art (title, raw, created_ms) VALUES (?1, ?2, ?3)",
            params![title, raw, now_ms],
        )?;
        Ok(AsciiArt {
            id: self.conn().last_insert_rowid(),
            title: title.to_string(),
            raw: raw.to_string(),
            created_ms: now_ms,
        })
    }

    /// Deletes a piece; `false` when no piece had that id.
    pub fn ascii_delete(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM ascii_art WHERE id = ?1", params![id])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_note_is_no_note_and_a_long_one_is_cut_on_a_char_boundary() {
        assert_eq!(normalize_note(""), None);
        assert_eq!(normalize_note(" \n\t "), None);
        assert_eq!(normalize_note(" hi\n").as_deref(), Some(" hi\n"));
        let long = "é".repeat(NOTE_MAX_CHARS + 5);
        let cut = normalize_note(&long).unwrap();
        assert_eq!(cut.chars().count(), NOTE_MAX_CHARS);
    }

    #[test]
    fn ascii_titles_are_trimmed_and_cut_and_big_or_blank_art_is_refused() {
        let (title, raw) =
            check_ascii("  a title that is far longer than forty characters  ", "x").unwrap();
        assert_eq!(title.chars().count(), ASCII_TITLE_MAX_CHARS);
        assert!(title.starts_with("a title"));
        assert_eq!(raw, "x");
        assert_eq!(check_ascii("t", "  \n "), Err(AsciiRefusal::Empty));
        assert!(check_ascii("t", &"#".repeat(ASCII_RAW_MAX_BYTES)).is_ok());
        assert_eq!(
            check_ascii("t", &"#".repeat(ASCII_RAW_MAX_BYTES + 1)),
            Err(AsciiRefusal::TooLarge)
        );
    }
}
