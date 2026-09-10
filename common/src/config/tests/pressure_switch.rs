use super::*;

#[test]
fn any_hold_needs_one_occupied_plate() {
    assert!(!SwitchHold::Any.is_held(3, 0, 2));
    assert!(SwitchHold::Any.is_held(3, 1, 2));
    assert!(
        SwitchHold::Any.is_held(1, 1, 0),
        "a dead presser still counts under any"
    );
}

#[test]
fn everyone_hold_needs_every_player_when_plates_suffice() {
    assert!(!SwitchHold::Everyone.is_held(3, 1, 2));
    assert!(SwitchHold::Everyone.is_held(3, 2, 2));
    assert!(SwitchHold::Everyone.is_held(1, 1, 1));
}

#[test]
fn everyone_hold_needs_every_plate_when_players_outnumber_them() {
    assert!(!SwitchHold::Everyone.is_held(2, 1, 5));
    assert!(SwitchHold::Everyone.is_held(2, 2, 5));
}

#[test]
fn everyone_hold_never_holds_without_plates_or_players() {
    assert!(!SwitchHold::Everyone.is_held(0, 0, 3));
    assert!(!SwitchHold::Everyone.is_held(2, 0, 0));
}
