-- Migration 4 — R189 (user 2026-10-22): the v2 redesign's own data.
-- Migrations 1–3 are frozen; an earlier build's DB has run them and takes
-- only this file. Applied by `db::open_db` inside a transaction, guarded by
-- `PRAGMA user_version`. Nothing here touches an existing table, so every
-- stored match, name and RR row survives the upgrade as it was.

-- Players the user saved (the Saved screen, the "saved player in your
-- match" alert). One row per saved puuid; unsaving deletes it. `saved_ms`
-- orders the list, newest first.
CREATE TABLE saved_players (
    puuid TEXT PRIMARY KEY,
    saved_ms INTEGER NOT NULL
);

-- The user's private note on a player (the user's own puuid holds their
-- personal notes). A row exists only for a non-empty note: clearing a note
-- deletes it.
CREATE TABLE player_notes (
    puuid TEXT PRIMARY KEY,
    note TEXT NOT NULL,
    updated_ms INTEGER NOT NULL
);

-- Players whose smurf flag the user dismissed ("Not a smurf, hide flag").
CREATE TABLE smurf_dismissed (
    puuid TEXT PRIMARY KEY,
    dismissed_ms INTEGER NOT NULL
);

-- The ASCII studio's saved gallery, newest first by `created_ms`.
-- AUTOINCREMENT: a deleted item's id is never handed to a later one, so a
-- stale delete from the UI can never remove a newer piece.
CREATE TABLE ascii_art (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    raw TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
