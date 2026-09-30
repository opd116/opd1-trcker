-- Migration 2 — the Phase 2 columns (R89, design v8's Player Details and
-- Match Details). Migration 1 (`schema.sql`) is frozen as it stood at
-- `c9ad7f4`; an earlier build's DB has run it and takes only this file.
-- Applied by `db::open_db` inside a transaction, guarded by
-- `PRAGMA user_version`. Every column is NULL on rows stored before this
-- migration (and for any payload that lacks the value).

-- `matchInfo.gameLengthMillis`: how long the game ran.
ALTER TABLE matches ADD COLUMN game_length_ms INTEGER;

-- Rounds won in rounds `roundNum` 0..=11 (the first half), counted from
-- `roundResults[].winningTeam`. Filled only for competitive, unrated and
-- premier (the queues with 12-round halves); NULL otherwise.
ALTER TABLE match_teams ADD COLUMN rounds_won_h1 INTEGER;

-- `players[].competitiveTier`: Riot's rank for the player DURING the
-- match (R81); 0 when unranked.
ALTER TABLE match_players ADD COLUMN competitive_tier INTEGER;

-- Σ `roundResults[].playerStats[subject].damage[].damage`; ADR is this
-- over the rounds played.
ALTER TABLE match_players ADD COLUMN damage INTEGER;
