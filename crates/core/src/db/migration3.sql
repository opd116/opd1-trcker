-- Migration 3 — R150: asking name-service outside live stages.
-- Migrations 1 and 2 are frozen; an earlier build's DB has run them and
-- takes only this file. Applied by `db::open_db` inside a transaction,
-- guarded by `PRAGMA user_version`.

-- The "no name" marker: how far a name-service question about this player
-- has already ended blank. NULL: never asked (every row before this
-- migration). Otherwise the start (epoch ms) of the newest finished match
-- whose question got a blank answer, or 0 for a blank answer to an
-- on-demand question (Player Details, Match Details, Top 3). Name-service
-- answers with the player's status *now*, so a question raised by a match
-- no newer than the marker adds nothing; the next, newer finished match asks
-- again. No timer. One INTEGER on `players`, a row the name lookups already
-- read by primary key: no table, no index.
ALTER TABLE players ADD COLUMN name_asked_ms INTEGER;
