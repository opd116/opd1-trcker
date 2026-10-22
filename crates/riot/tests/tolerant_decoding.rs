//! R166: tolerant decoding. Riot's answers sometimes omit a field or send it as
//! `null`; like Scout (`dict.get(...) or default`), a surprise in one field must
//! never fail a whole answer. The exceptions are the fields that ARE an
//! element's identity (a match id, a subject): an element without one is
//! dropped, never invented. Where a default would be wrong data downstream
//! (a rank of 0 RR, a match dated 1970, a hidden player shown as visible), the
//! field is optional or fails closed, and the converters skip or ignore it.
//!
//! Every case below runs the same two variants of one field: absent, and an
//! explicit `null`. Bodies use invented ids.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use opd1_riot::endpoints::{
    CompetitiveUpdatesResponse, CoreGameLoadoutsResponse, CoreGameMatchResponse,
    MatchDetailsResponse, MatchHistoryResponse, MatchRef, OwnedItemsResponse, PartyOfResponse,
    PartyResponse, PlayerLoadoutResponse, PlayerMmrResponse, PregameLoadoutsResponse,
    PregameMatchResponse, PresencesResponse, SKIN_LEVEL_ITEM_TYPE,
};
use opd1_riot::to_core::{
    comp_update_rows_from_response, current_rank_from_mmr, seasonal_map_from_mmr,
};

const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const ACT: &str = "dddddddd-0000-0000-0000-000000000001";

/// `base` with the key at `pointer` (`/a/0/b`: the last segment is the key)
/// removed, and with it set to `null`: the two variants every field gets.
fn absent_and_null(base: &Value, pointer: &str) -> [(&'static str, Value); 2] {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer has a key");
    let mut absent = base.clone();
    let obj = if parent.is_empty() {
        &mut absent
    } else {
        absent.pointer_mut(parent).expect("parent exists")
    };
    obj.as_object_mut().unwrap().remove(key);
    let mut null = base.clone();
    let obj = if parent.is_empty() {
        &mut null
    } else {
        null.pointer_mut(parent).expect("parent exists")
    };
    obj.as_object_mut()
        .unwrap()
        .insert(key.to_string(), Value::Null);
    [("absent", absent), ("null", null)]
}

fn decode<T: DeserializeOwned>(what: &str, value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{what}: {e}"))
}

// ---------------------------------------------------------------------
// competitive updates
// ---------------------------------------------------------------------

fn comp_row(id: &str) -> Value {
    json!({
        "MatchID": id, "MapID": "/Game/Maps/Ascent/Ascent", "SeasonID": ACT,
        "MatchStartTime": 1_790_000_000_000i64,
        "TierAfterUpdate": 15, "TierBeforeUpdate": 14,
        "RankedRatingAfterUpdate": 42, "RankedRatingBeforeUpdate": 20,
        "RankedRatingEarned": 22,
    })
}

fn comp_body() -> Value {
    json!({"Subject": ME, "Matches": [comp_row("m1"), comp_row("m2")]})
}

#[test]
fn comp_updates_missing_or_null_matches_is_an_empty_list() {
    for field in ["/Matches", "/Subject"] {
        for (how, body) in absent_and_null(&comp_body(), field) {
            let r: CompetitiveUpdatesResponse = decode(&format!("{field} {how}"), &body);
            let want = if field == "/Matches" { 0 } else { 2 };
            assert_eq!(r.matches.len(), want, "{field} {how}");
        }
    }
}

#[test]
fn comp_update_rows_keep_their_place_when_a_soft_field_is_missing() {
    // Map, season and the before-values are optional: the row stays.
    for field in [
        "MapID",
        "SeasonID",
        "TierBeforeUpdate",
        "RankedRatingBeforeUpdate",
    ] {
        let pointer = format!("/Matches/0/{field}");
        for (how, body) in absent_and_null(&comp_body(), &pointer) {
            let r: CompetitiveUpdatesResponse = decode(&format!("{field} {how}"), &body);
            assert_eq!(r.matches.len(), 2, "{field} {how}");
            let rows = comp_update_rows_from_response(&r);
            assert_eq!(rows.len(), 2, "{field} {how}");
            match field {
                "MapID" => assert_eq!(rows[0].map_id, None, "{how}"),
                "SeasonID" => assert_eq!(rows[0].season_id, "", "{how}"),
                "TierBeforeUpdate" => assert_eq!(rows[0].tier_before, None, "{how}"),
                _ => {}
            }
        }
    }
}

#[test]
fn a_comp_update_row_without_what_makes_it_a_row_fails_the_whole_answer() {
    // No id, start time, tier or RR: a default 0 would be a 1970 match, an
    // unranked tier or "+0 RR", and dropping the row would shift "newest" (the
    // stored-rank fallback would show the row before it as the current rank).
    // So the answer fails and the existing retry paths ask again.
    for field in [
        "MatchID",
        "MatchStartTime",
        "TierAfterUpdate",
        "RankedRatingAfterUpdate",
        "RankedRatingEarned",
    ] {
        let pointer = format!("/Matches/0/{field}");
        for (how, body) in absent_and_null(&comp_body(), &pointer) {
            assert!(
                serde_json::from_value::<CompetitiveUpdatesResponse>(body).is_err(),
                "{field} {how}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// match history
// ---------------------------------------------------------------------

fn history_body() -> Value {
    json!({"Subject": ME, "Total": 2, "History": [
        {"MatchID": "m1", "GameStartTime": 1_790_000_000_000i64, "QueueID": "competitive"},
        {"MatchID": "m2", "GameStartTime": 1_790_000_100_000i64, "QueueID": "unrated"},
    ]})
}

#[test]
fn match_history_tolerates_a_missing_list_subject_and_queue() {
    for (how, body) in absent_and_null(&history_body(), "/History") {
        let r: MatchHistoryResponse = decode(&format!("History {how}"), &body);
        assert!(r.history.is_empty(), "{how}");
    }
    for (how, body) in absent_and_null(&history_body(), "/Subject") {
        let r: MatchHistoryResponse = decode(&format!("Subject {how}"), &body);
        assert_eq!(r.history.len(), 2, "{how}");
    }
    for (how, body) in absent_and_null(&history_body(), "/History/0/QueueID") {
        let r: MatchHistoryResponse = decode(&format!("QueueID {how}"), &body);
        assert_eq!(r.history.len(), 2, "{how}");
        assert_eq!(r.history[0].queue_id, "", "{how}");
    }
}

#[test]
fn a_history_entry_without_an_id_or_a_start_fails_the_whole_answer() {
    // Dropping the newest entry would make the real second one "most recent"
    // for party detection; the answer fails instead.
    for field in ["MatchID", "GameStartTime"] {
        let pointer = format!("/History/0/{field}");
        for (how, body) in absent_and_null(&history_body(), &pointer) {
            assert!(
                serde_json::from_value::<MatchHistoryResponse>(body).is_err(),
                "{field} {how}"
            );
        }
    }
    // A list that is not a list is no answer either (a null one is empty).
    let mut body = history_body();
    body["History"] = json!("garbage");
    assert!(serde_json::from_value::<MatchHistoryResponse>(body).is_err());
}

// ---------------------------------------------------------------------
// mmr
// ---------------------------------------------------------------------

fn mmr_body() -> Value {
    json!({"Subject": ME, "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {
        ACT: {
            "SeasonID": ACT, "CompetitiveTier": 15, "RankedRating": 42,
            "NumberOfWins": 6, "NumberOfWinsWithPlacements": 7, "NumberOfGames": 10,
            "WinsByTier": {"15": 4}, "LeaderboardRank": 0
        }
    }}}})
}

fn act_ptr(field: &str) -> String {
    format!("/QueueSkills/competitive/SeasonalInfoBySeasonID/{ACT}/{field}")
}

#[test]
fn a_season_entry_with_a_missing_field_still_decodes() {
    for field in [
        "SeasonID",
        "CompetitiveTier",
        "RankedRating",
        "NumberOfWins",
        "NumberOfWinsWithPlacements",
        "NumberOfGames",
        "WinsByTier",
        "LeaderboardRank",
    ] {
        for (how, body) in absent_and_null(&mmr_body(), &act_ptr(field)) {
            let r: PlayerMmrResponse = decode(&format!("{field} {how}"), &body);
            let map = seasonal_map_from_mmr(&r);
            assert!(map.contains_key(ACT), "{field} {how}: keyed by the map key");
        }
    }
}

#[test]
fn a_season_entry_without_a_rank_or_rr_gives_no_current_rank_never_zero_rr() {
    // Missing RR: a tier-15 rank with "0 RR" would be wrong data; no current
    // rank from mmr lets the stored competitive updates answer (R154).
    for field in ["RankedRating", "CompetitiveTier"] {
        for (how, body) in absent_and_null(&mmr_body(), &act_ptr(field)) {
            let r: PlayerMmrResponse = decode(&format!("{field} {how}"), &body);
            assert_eq!(current_rank_from_mmr(&r, ACT), None, "{field} {how}");
        }
    }
    // Everything else missing leaves the rank intact.
    for field in ["SeasonID", "NumberOfWins", "NumberOfGames", "WinsByTier"] {
        for (how, body) in absent_and_null(&mmr_body(), &act_ptr(field)) {
            let r: PlayerMmrResponse = decode(&format!("{field} {how}"), &body);
            let rank = current_rank_from_mmr(&r, ACT).unwrap_or_else(|| panic!("{field} {how}"));
            assert_eq!((rank.tier, rank.rr), (15, 42), "{field} {how}");
        }
    }
}

#[test]
fn a_missing_games_or_wins_count_is_an_unknown_record_never_a_zero_percent() {
    // The model reads 0 games as "no record" (win rate, wins and games show
    // `—`); a missing count must never become "0 wins of 10 games".
    let record = |body: &Value| {
        let r: PlayerMmrResponse = decode("record", body);
        let info = seasonal_map_from_mmr(&r)[ACT].clone();
        (info.number_of_wins, info.number_of_games)
    };
    assert_eq!(record(&mmr_body()), (7, 10), "placements wins count");
    for (how, body) in absent_and_null(&mmr_body(), &act_ptr("NumberOfGames")) {
        assert_eq!(record(&body), (0, 0), "games {how}");
    }
    for (how, body) in absent_and_null(&mmr_body(), &act_ptr("NumberOfWinsWithPlacements")) {
        assert_eq!(
            record(&body),
            (6, 10),
            "placements {how}: NumberOfWins stands in"
        );
    }
    for (how, body) in absent_and_null(&mmr_body(), &act_ptr("NumberOfWins")) {
        assert_eq!(
            record(&body),
            (7, 10),
            "wins {how}: placements wins stand in"
        );
        // Neither wins figure: unknown, not zero.
        let mut none = body.clone();
        none["QueueSkills"]["competitive"]["SeasonalInfoBySeasonID"][ACT]
            .as_object_mut()
            .unwrap()
            .remove("NumberOfWinsWithPlacements");
        assert_eq!(record(&none), (0, 0), "no wins at all, {how}");
    }
    // The rank of that entry is untouched.
    for (how, body) in absent_and_null(&mmr_body(), &act_ptr("NumberOfGames")) {
        let r: PlayerMmrResponse = decode("rank", &body);
        assert_eq!(current_rank_from_mmr(&r, ACT).unwrap().tier, 15, "{how}");
    }
}

#[test]
fn a_null_season_or_queue_entry_is_dropped_the_others_stay() {
    let mut body = mmr_body();
    let seasons = body
        .pointer_mut("/QueueSkills/competitive/SeasonalInfoBySeasonID")
        .unwrap()
        .as_object_mut()
        .unwrap();
    seasons.insert("eeeeeeee-0000-0000-0000-000000000002".into(), Value::Null);
    body["QueueSkills"]
        .as_object_mut()
        .unwrap()
        .insert("unrated".into(), Value::Null);
    let r: PlayerMmrResponse = decode("null season and queue", &body);
    let map = seasonal_map_from_mmr(&r);
    assert_eq!(map.len(), 1);
    assert!(map.contains_key(ACT));
}

#[test]
fn a_missing_subject_in_mmr_is_fine() {
    for (how, body) in absent_and_null(&mmr_body(), "/Subject") {
        let r: PlayerMmrResponse = decode(&format!("Subject {how}"), &body);
        assert_eq!(seasonal_map_from_mmr(&r).len(), 1, "{how}");
    }
}

// ---------------------------------------------------------------------
// pregame / core-game / party identities
// ---------------------------------------------------------------------

fn identity() -> Value {
    json!({"Incognito": false, "HideAccountLevel": false, "AccountLevel": 187,
           "PlayerCardID": "card-1"})
}

fn pregame_body() -> Value {
    json!({
        "MapID": "/Game/Maps/Ascent/Ascent",
        "Teams": [{"TeamID": "Blue", "Players": [
            {"Subject": ME, "CharacterID": "agent-1", "CharacterSelectionState": "locked",
             "PlayerIdentity": identity(), "CompetitiveTier": 15},
            {"Subject": "p2", "CharacterID": "", "CharacterSelectionState": "",
             "PlayerIdentity": identity(), "CompetitiveTier": 0},
        ]}],
        "AllyTeam": {"TeamID": "Blue", "Players": [
            {"Subject": ME, "CharacterID": "agent-1", "CharacterSelectionState": "locked",
             "PlayerIdentity": identity()},
        ]},
        "EnemyTeam": null,
    })
}

#[test]
fn pregame_match_tolerates_a_missing_or_null_field_everywhere() {
    for field in [
        "/MapID",
        "/Teams",
        "/AllyTeam",
        "/EnemyTeam",
        "/Teams/0/TeamID",
        "/Teams/0/Players",
        "/AllyTeam/TeamID",
        "/AllyTeam/Players",
        "/Teams/0/Players/0/CharacterID",
        "/Teams/0/Players/0/CharacterSelectionState",
        "/Teams/0/Players/0/PlayerIdentity",
        "/Teams/0/Players/0/CompetitiveTier",
    ] {
        for (how, body) in absent_and_null(&pregame_body(), field) {
            let _: PregameMatchResponse = decode(&format!("{field} {how}"), &body);
        }
    }
    let r: PregameMatchResponse = decode("EnemyTeam null", &pregame_body());
    assert!(r.enemy_team.is_none());
}

#[test]
fn a_malformed_team_object_is_no_team_not_a_failed_answer() {
    let mut body = pregame_body();
    body["AllyTeam"] = json!("garbage");
    let r: PregameMatchResponse = decode("garbage ally", &body);
    assert!(r.ally_team.is_none());
    assert_eq!(r.teams.len(), 1);
}

#[test]
fn a_pregame_player_without_a_subject_is_dropped() {
    for (how, body) in absent_and_null(&pregame_body(), "/Teams/0/Players/0/Subject") {
        let r: PregameMatchResponse = decode(&format!("Subject {how}"), &body);
        assert_eq!(r.teams[0].players.len(), 1, "{how}");
        assert_eq!(r.teams[0].players[0].subject, "p2", "{how}");
    }
}

#[test]
fn a_missing_player_identity_fails_closed_the_player_is_hidden() {
    // Privacy: "not known to be visible" is "hidden", so a name is never
    // asked for a player whose flag did not arrive.
    for (how, body) in absent_and_null(&pregame_body(), "/Teams/0/Players/0/PlayerIdentity") {
        let r: PregameMatchResponse = decode(&format!("identity {how}"), &body);
        let id = &r.teams[0].players[0].player_identity;
        assert!(id.incognito, "{how}");
        assert!(id.hide_account_level, "{how}: no level to show");
        assert_eq!(id.account_level, 0, "{how}");
    }
    for field in [
        "Incognito",
        "HideAccountLevel",
        "AccountLevel",
        "PlayerCardID",
    ] {
        let pointer = format!("/Teams/0/Players/0/PlayerIdentity/{field}");
        for (how, body) in absent_and_null(&pregame_body(), &pointer) {
            let r: PregameMatchResponse = decode(&format!("{field} {how}"), &body);
            let id = &r.teams[0].players[0].player_identity;
            match field {
                "Incognito" => assert!(id.incognito, "{how}: unknown is hidden"),
                "HideAccountLevel" => {
                    assert!(!id.incognito, "{how}");
                    assert!(!id.hide_account_level, "{how}: the level is there");
                    assert_eq!(id.account_level, 187, "{how}");
                }
                "AccountLevel" => {
                    assert!(id.hide_account_level, "{how}: no level, none shown");
                    assert_eq!(id.account_level, 0, "{how}");
                }
                _ => {
                    assert!(!id.incognito, "{how}");
                    assert_eq!(id.player_card_id, "", "{how}");
                }
            }
        }
    }
}

#[test]
fn pregame_loadouts_tolerate_missing_fields() {
    let body = json!({"Loadouts": [{"Subject": ME, "Items": {}}], "LoadoutsValid": true});
    for field in ["/Loadouts", "/LoadoutsValid"] {
        for (how, b) in absent_and_null(&body, field) {
            let r: PregameLoadoutsResponse = decode(&format!("{field} {how}"), &b);
            assert_eq!(
                r.loadouts.len(),
                usize::from(field == "/LoadoutsValid"),
                "{how}"
            );
            assert_eq!(r.loadouts_valid, field == "/Loadouts", "{how}");
        }
    }
}

fn coregame_body() -> Value {
    json!({"MapID": "/Game/Maps/Ascent/Ascent", "Players": [
        {"Subject": ME, "TeamID": "Blue", "CharacterID": "a1", "PlayerIdentity": identity()},
        {"Subject": "p2", "TeamID": "Red", "CharacterID": "a2", "PlayerIdentity": identity()},
    ]})
}

#[test]
fn core_game_match_tolerates_missing_fields_and_drops_a_subjectless_player() {
    for field in [
        "/MapID",
        "/Players",
        "/Players/0/TeamID",
        "/Players/0/CharacterID",
        "/Players/0/PlayerIdentity",
    ] {
        for (how, body) in absent_and_null(&coregame_body(), field) {
            let r: CoreGameMatchResponse = decode(&format!("{field} {how}"), &body);
            assert_eq!(
                r.players.len(),
                if field == "/Players" { 0 } else { 2 },
                "{field} {how}"
            );
        }
    }
    for (how, body) in absent_and_null(&coregame_body(), "/Players/0/Subject") {
        let r: CoreGameMatchResponse = decode(&format!("Subject {how}"), &body);
        assert_eq!(r.players.len(), 1, "{how}");
    }
}

#[test]
fn core_game_loadouts_tolerate_missing_fields() {
    let body =
        json!({"Loadouts": [{"CharacterID": "a1", "Loadout": {"Subject": ME, "Items": {}}}]});
    for field in [
        "/Loadouts",
        "/Loadouts/0/CharacterID",
        "/Loadouts/0/Loadout",
    ] {
        for (how, b) in absent_and_null(&body, field) {
            let r: CoreGameLoadoutsResponse = decode(&format!("{field} {how}"), &b);
            assert_eq!(
                r.loadouts.len(),
                usize::from(field != "/Loadouts"),
                "{field} {how}"
            );
        }
    }
}

#[test]
fn match_ref_needs_its_match_id_and_nothing_else() {
    let body = json!({"Subject": ME, "MatchID": "m1"});
    for (how, b) in absent_and_null(&body, "/Subject") {
        let r: MatchRef = decode(&format!("Subject {how}"), &b);
        assert_eq!(r.match_id, "m1", "{how}");
    }
    for (how, b) in absent_and_null(&body, "/MatchID") {
        assert!(
            serde_json::from_value::<MatchRef>(b).is_err(),
            "MatchID {how}"
        );
    }
}

fn party_body() -> Value {
    json!({"ID": "party-1", "State": "DEFAULT", "EligibleQueues": ["competitive"], "Members": [
        {"Subject": ME, "PlayerIdentity": identity(), "IsOwner": true, "IsReady": true,
         "CompetitiveTier": 15},
        {"Subject": "p2", "PlayerIdentity": identity(), "IsOwner": false, "IsReady": true,
         "CompetitiveTier": 12},
    ]})
}

#[test]
fn party_tolerates_missing_fields_and_drops_a_subjectless_member() {
    for field in [
        "/ID",
        "/State",
        "/EligibleQueues",
        "/Members",
        "/Members/0/PlayerIdentity",
        "/Members/0/IsOwner",
        "/Members/0/IsReady",
        "/Members/0/CompetitiveTier",
    ] {
        for (how, body) in absent_and_null(&party_body(), field) {
            let r: PartyResponse = decode(&format!("{field} {how}"), &body);
            assert_eq!(
                r.members.len(),
                if field == "/Members" { 0 } else { 2 },
                "{field} {how}"
            );
        }
    }
    for (how, body) in absent_and_null(&party_body(), "/Members/0/Subject") {
        let r: PartyResponse = decode(&format!("Subject {how}"), &body);
        assert_eq!(r.members.len(), 1, "{how}");
    }
    // A member whose identity did not arrive is hidden, not visible.
    for (how, body) in absent_and_null(&party_body(), "/Members/0/PlayerIdentity") {
        let r: PartyResponse = decode(&format!("identity {how}"), &body);
        assert!(r.members[0].player_identity.incognito, "{how}");
    }
}

#[test]
fn party_of_needs_the_party_id() {
    let body = json!({"Subject": ME, "CurrentPartyID": "party-1"});
    for (how, b) in absent_and_null(&body, "/Subject") {
        let r: PartyOfResponse = decode(&format!("Subject {how}"), &b);
        assert_eq!(r.current_party_id, "party-1", "{how}");
    }
    for (how, b) in absent_and_null(&body, "/CurrentPartyID") {
        assert!(
            serde_json::from_value::<PartyOfResponse>(b).is_err(),
            "{how}"
        );
    }
}

#[test]
fn presences_tolerate_missing_fields() {
    let body = json!({"presences": [
        {"puuid": ME, "product": "valorant", "private": "abc"},
        {"puuid": "p2", "product": "league_of_legends", "private": null},
    ]});
    for (how, b) in absent_and_null(&body, "/presences") {
        let r: PresencesResponse = decode(&format!("presences {how}"), &b);
        assert!(r.presences.is_empty(), "{how}");
    }
    for (how, b) in absent_and_null(&body, "/presences/0/product") {
        let r: PresencesResponse = decode(&format!("product {how}"), &b);
        assert_eq!(r.presences.len(), 2, "{how}");
        assert_eq!(r.presences[0].product, "", "{how}");
    }
    for (how, b) in absent_and_null(&body, "/presences/0/private") {
        let r: PresencesResponse = decode(&format!("private {how}"), &b);
        assert_eq!(r.presences[0].private, None, "{how}");
    }
    for (how, b) in absent_and_null(&body, "/presences/0/puuid") {
        let r: PresencesResponse = decode(&format!("puuid {how}"), &b);
        assert_eq!(
            r.presences.len(),
            1,
            "a presence with no puuid is dropped: {how}"
        );
    }
}

// ---------------------------------------------------------------------
// the user's own loadout
// ---------------------------------------------------------------------

#[test]
fn player_loadout_tolerates_missing_fields_and_drops_an_id_less_gun() {
    let body = json!({"Subject": ME, "Guns": [
        {"ID": "gun-1", "SkinID": "s1", "ChromaID": "c1"},
        {"ID": "gun-2", "SkinID": "s2", "ChromaID": "c2"},
    ]});
    for (how, b) in absent_and_null(&body, "/Subject") {
        let r: PlayerLoadoutResponse = decode(&format!("Subject {how}"), &b);
        assert_eq!(r.guns.len(), 2, "{how}");
    }
    for field in ["SkinID", "ChromaID"] {
        for (how, b) in absent_and_null(&body, &format!("/Guns/0/{field}")) {
            let r: PlayerLoadoutResponse = decode(&format!("{field} {how}"), &b);
            assert_eq!(r.guns.len(), 2, "{field} {how}");
        }
    }
    for (how, b) in absent_and_null(&body, "/Guns/0/ID") {
        let r: PlayerLoadoutResponse = decode(&format!("ID {how}"), &b);
        assert_eq!(r.guns.len(), 1, "{how}");
        assert_eq!(r.guns[0].id, "gun-2", "{how}");
    }
}

// ---------------------------------------------------------------------
// match details (stored forever: identity and start stay required)
// ---------------------------------------------------------------------

fn details_body() -> Value {
    json!({
        "matchInfo": {"matchId": "m1", "mapId": "/Game/Maps/Ascent/Ascent",
            "gameStartMillis": 1_790_000_000_000i64, "queueID": "competitive",
            "isRanked": true, "completionState": "Completed", "gameLengthMillis": 1_800_000},
        "players": [
            {"subject": ME, "gameName": "A", "tagLine": "T", "teamId": "Blue", "partyId": "p",
             "characterId": "a1", "competitiveTier": 15,
             "stats": {"score": 100, "roundsPlayed": 20, "kills": 10, "deaths": 5, "assists": 2}},
            {"subject": "p2", "gameName": "B", "tagLine": "U", "teamId": "Red", "partyId": "q",
             "characterId": "a2", "stats": null},
        ],
        "teams": [{"teamId": "Blue", "won": true, "roundsWon": 13},
                  {"teamId": "Red", "won": false, "roundsWon": 7}],
        "roundResults": [{"roundNum": 0, "winningTeam": "Blue", "playerStats": [
            {"subject": ME, "damage": [
                {"receiver": "p2", "damage": 150, "legshots": 0, "bodyshots": 1, "headshots": 1}]}]}],
    })
}

#[test]
fn match_details_tolerates_soft_fields_missing_or_null() {
    for field in [
        "/matchInfo/mapId",
        "/matchInfo/completionState",
        "/matchInfo/gameLengthMillis",
        "/players/0/gameName",
        "/players/0/tagLine",
        "/players/0/partyId",
        "/players/0/characterId",
        "/players/0/competitiveTier",
        "/players/0/stats",
        "/teams",
        "/teams/0/roundsWon",
        "/roundResults",
        "/roundResults/0/roundNum",
        "/roundResults/0/winningTeam",
        "/roundResults/0/playerStats",
        "/roundResults/0/playerStats/0/damage",
        "/roundResults/0/playerStats/0/damage/0/receiver",
        "/roundResults/0/playerStats/0/damage/0/damage",
        "/roundResults/0/playerStats/0/damage/0/legshots",
        "/roundResults/0/playerStats/0/damage/0/bodyshots",
        "/roundResults/0/playerStats/0/damage/0/headshots",
    ] {
        for (how, body) in absent_and_null(&details_body(), field) {
            let r: MatchDetailsResponse = decode(&format!("{field} {how}"), &body);
            assert_eq!(r.players.len(), 2, "{field} {how}");
        }
    }
}

#[test]
fn a_malformed_stats_block_never_fails_the_match() {
    // A stats block with a hole is "no stats", never zero kills stored forever.
    for field in ["score", "roundsPlayed", "kills", "deaths", "assists"] {
        for (how, body) in absent_and_null(&details_body(), &format!("/players/0/stats/{field}")) {
            let r: MatchDetailsResponse = decode(&format!("{field} {how}"), &body);
            assert!(r.players[0].stats.is_none(), "{field} {how}");
        }
    }
}

#[test]
fn a_match_whose_player_has_no_team_or_whose_team_has_no_result_is_not_decoded() {
    // Stored for good, and read by party detection (same team id), encounters
    // and the result: a "" team id would make strangers teammates, and a
    // dropped winner would be stored as a draw. So the answer fails and the
    // fetch is retried like any failure.
    for field in ["subject", "teamId"] {
        for (how, body) in absent_and_null(&details_body(), &format!("/players/0/{field}")) {
            assert!(
                serde_json::from_value::<MatchDetailsResponse>(body).is_err(),
                "player {field} {how}"
            );
        }
    }
    for field in ["teamId", "won"] {
        for (how, body) in absent_and_null(&details_body(), &format!("/teams/0/{field}")) {
            assert!(
                serde_json::from_value::<MatchDetailsResponse>(body).is_err(),
                "team {field} {how}"
            );
        }
    }
}

#[test]
fn match_details_keeps_what_a_stored_match_cannot_do_without() {
    // The match row is kept for good: no id, start or queue means no row.
    for field in [
        "/matchInfo",
        "/matchInfo/matchId",
        "/matchInfo/gameStartMillis",
        "/matchInfo/queueID",
        "/players",
    ] {
        for (how, body) in absent_and_null(&details_body(), field) {
            assert!(
                serde_json::from_value::<MatchDetailsResponse>(body).is_err(),
                "{field} {how} must fail the answer"
            );
        }
    }
}

// ---------------------------------------------------------------------
// owned items (R189)
// ---------------------------------------------------------------------

#[test]
fn owned_items_tolerate_missing_fields_and_drop_bad_elements() {
    const L1: &str = "aaaaaaaa-1111-0000-0000-000000000001";
    const L2: &str = "aaaaaaaa-1111-0000-0000-000000000002";
    let body = json!({"ItemTypeID": SKIN_LEVEL_ITEM_TYPE, "Entitlements": [
        {"TypeID": SKIN_LEVEL_ITEM_TYPE, "ItemID": L1},
        {"TypeID": SKIN_LEVEL_ITEM_TYPE, "ItemID": L2},
    ]});
    for (how, b) in absent_and_null(&body, "/ItemTypeID") {
        let r: OwnedItemsResponse = decode(&format!("ItemTypeID {how}"), &b);
        assert_eq!(r.item_ids(SKIN_LEVEL_ITEM_TYPE), [L1, L2], "{how}");
    }
    for (how, b) in absent_and_null(&body, "/Entitlements/0/TypeID") {
        let r: OwnedItemsResponse = decode(&format!("TypeID {how}"), &b);
        assert_eq!(r.item_ids(SKIN_LEVEL_ITEM_TYPE), [L1, L2], "{how}");
    }
    // An element without its item id is dropped, never invented.
    for (how, b) in absent_and_null(&body, "/Entitlements/0/ItemID") {
        let r: OwnedItemsResponse = decode(&format!("ItemID {how}"), &b);
        assert_eq!(r.item_ids(SKIN_LEVEL_ITEM_TYPE), [L2], "{how}");
    }
    for (how, b) in absent_and_null(&body, "/Entitlements") {
        let r: OwnedItemsResponse = decode(&format!("Entitlements {how}"), &b);
        assert!(r.item_ids(SKIN_LEVEL_ITEM_TYPE).is_empty(), "{how}");
    }
    // Wrong kinds and shapes: no ids, never a failed answer; ids that are
    // not uuids (a path, a number) are never kept.
    for odd in [
        json!({}),
        json!({"Entitlements": "x"}),
        json!({"Entitlements": [1, "x", {"ItemID": 5}, {"ItemID": "../x"}]}),
        json!({"EntitlementsByTypes": null}),
        json!({"EntitlementsByTypes": [{"ItemTypeID": null, "Entitlements": null}]}),
        json!({"ItemTypeID": "other-type", "Entitlements": [{"ItemID": L1}]}),
        json!({"Entitlements": [{"TypeID": "other-type", "ItemID": L1}]}),
    ] {
        let r: OwnedItemsResponse = decode(&format!("{odd}"), &odd);
        assert!(r.item_ids(SKIN_LEVEL_ITEM_TYPE).is_empty(), "{odd}");
    }
}
