//! `VisiblePuuid` is the type-level guarantee behind CLAUDE.md's "never
//! request a hidden player's name from any endpoint": the only constructor
//! returns `None` for an incognito player, so a hidden PUUID can never
//! reach `RiotSession::names`.

use opd1_riot::endpoints::VisiblePuuid;

#[test]
fn new_returns_some_for_a_visible_player() {
    let visible = VisiblePuuid::new("aaaaaaaa-0000-0000-0000-000000000001".to_string(), false);
    assert!(visible.is_some());
    assert_eq!(
        visible.unwrap().as_str(),
        "aaaaaaaa-0000-0000-0000-000000000001"
    );
}

#[test]
fn new_returns_none_for_an_incognito_player() {
    let hidden = VisiblePuuid::new("bbbbbbbb-0000-0000-0000-000000000001".to_string(), true);
    assert!(hidden.is_none());
}
