-- Migration 1 — initial schema.
--
-- Column choices follow CLAUDE.md's "Smart data (SQLite)": players are
-- keyed by PUUID (recognized across name changes); match details are
-- immutable and stored as compact extracted columns, never raw JSON
-- (the one exception is `mmr_snapshots.json`, ruling R29). Applied by
-- `db::open_db` inside a transaction, guarded by `PRAGMA user_version`.

-- One row per player ever seen, upserted whenever we learn their current
-- name/level/card (from a live presence, match-details row, etc.).
CREATE TABLE players (
    puuid TEXT PRIMARY KEY,
    game_name TEXT NOT NULL,
    tag_line TEXT NOT NULL,
    account_level INTEGER NOT NULL,
    card_id TEXT NOT NULL,
    updated_ms INTEGER NOT NULL
);

-- One row per match, fetched once from match-details and never refetched.
-- `winning_team` is NULL for a draw / no team winner (ruling R29); the
-- per-team detail (rounds won, etc.) lives in `match_teams`.
CREATE TABLE matches (
    match_id TEXT PRIMARY KEY,
    map_id TEXT NOT NULL,
    queue_id TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    winning_team TEXT,
    is_ranked INTEGER NOT NULL,
    rounds_played INTEGER
);

-- Per-team result for a match (ruling R29). `won` mirrors match-details'
-- `teams[].won`; `winning_team` on `matches` is redundant with this but
-- kept for cheap single-column lookups.
CREATE TABLE match_teams (
    match_id TEXT NOT NULL REFERENCES matches(match_id),
    team_id TEXT NOT NULL,
    won INTEGER NOT NULL,
    rounds_won INTEGER,
    PRIMARY KEY (match_id, team_id)
);

-- Per-player compact stat line for a match. `game_name`/`tag_line` are
-- the name *as shown in that match's own details* (used by `known_name`
-- and post-match name resolution) — never fetched from name-service for
-- an incognito player.
CREATE TABLE match_players (
    match_id TEXT NOT NULL REFERENCES matches(match_id),
    puuid TEXT NOT NULL,
    team_id TEXT NOT NULL,
    party_id TEXT,
    agent_id TEXT,
    kills INTEGER NOT NULL,
    deaths INTEGER NOT NULL,
    assists INTEGER NOT NULL,
    score INTEGER NOT NULL,
    rounds INTEGER,
    hs INTEGER NOT NULL,
    bs INTEGER NOT NULL,
    ls INTEGER NOT NULL,
    game_name TEXT,
    tag_line TEXT,
    PRIMARY KEY (match_id, puuid)
);

-- A cheap cache of a player's match-history listing, used to diff against
-- `matches` (`new_match_ids`) instead of refetching everything every poll
-- ("sync, don't refetch" in CLAUDE.md).
CREATE TABLE history_index (
    puuid TEXT NOT NULL,
    match_id TEXT NOT NULL,
    queue_id TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    PRIMARY KEY (puuid, match_id)
);

-- One row per (puuid, competitive match) from `mmr/.../competitiveupdates`
-- — the RR/tier delta history that drives the rank-change and RR-chart
-- features. `tier_before` (ruling R29) lets a caller show the per-match
-- tier delta without re-deriving it from consecutive rows.
CREATE TABLE comp_updates (
    puuid TEXT NOT NULL,
    match_id TEXT NOT NULL,
    season_id TEXT NOT NULL,
    tier_after INTEGER NOT NULL,
    tier_before INTEGER,
    rr_after INTEGER NOT NULL,
    rr_earned INTEGER NOT NULL,
    map_id TEXT,
    start_ms INTEGER NOT NULL,
    PRIMARY KEY (puuid, match_id)
);

-- The single raw-JSON exception (ruling R29): the last-fetched MMR
-- response, kept whole so Task 5 doesn't need a second schema migration
-- every time it needs one more field out of it. `last_comp_match_id`
-- drives `needs_mmr_refresh`: no new competitive match means rank is
-- unchanged, so the MMR call can be skipped entirely.
CREATE TABLE mmr_snapshots (
    puuid TEXT PRIMARY KEY,
    json TEXT NOT NULL,
    last_comp_match_id TEXT,
    fetched_ms INTEGER NOT NULL
);

-- Small app-level key/value settings (ruling R31, Task 5b) — e.g.
-- `self_puuid`, so the Closed stage (VALORANT not running) can still show
-- the user's own card from the DB. Amended into migration 1 because no
-- user DB exists before tag v0.1.0; later additions need a new migration.
CREATE TABLE app_state (
    key TEXT PRIMARY KEY,
    value TEXT
);

CREATE INDEX idx_match_players_puuid ON match_players(puuid);
CREATE INDEX idx_history_index_puuid_start ON history_index(puuid, start_ms);
CREATE INDEX idx_comp_updates_puuid_start ON comp_updates(puuid, start_ms);
