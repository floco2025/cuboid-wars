use super::*;

#[test]
fn reached_chase_target_clears_chase_without_reacquire_cooldown() {
    let mut info = actor_info();
    info.go_to_position_is_chase = true;

    update_reached_go_to_state(&mut info);

    assert!(!info.go_to_position_is_chase);
    assert_eq!(info.chase_reacquire_timer, 0.0);
}

#[test]
fn reached_non_chase_target_does_not_start_reacquire_cooldown() {
    let mut info = actor_info();

    update_reached_go_to_state(&mut info);

    assert!(!info.go_to_position_is_chase);
    assert_eq!(info.chase_reacquire_timer, 0.0);
}

#[test]
fn reached_return_waypoint_advances_to_next_waypoint() {
    let mut info = actor_info();
    info.is_returning_to_spawn = true;
    info.return_path.push_back(Position { x: 2.0, y: 0.0, z: 2.0 });

    update_reached_go_to_state(&mut info);

    assert_eq!(info.go_to_position, Some(Position { x: 2.0, y: 0.0, z: 2.0 }));
    assert!(!info.go_to_position_is_chase);
    assert!(info.is_returning_to_spawn);
}

#[test]
fn reached_final_return_waypoint_clears_return_state() {
    let mut info = actor_info();
    info.is_returning_to_spawn = true;

    update_reached_go_to_state(&mut info);

    assert!(!info.go_to_position_is_chase);
    assert!(!info.is_returning_to_spawn);
}

#[test]
fn chasing_within_reach_holds_even_when_target_is_above() {
    let mut info = actor_info();
    info.go_to_position_is_chase = true;
    // Player on a ledge directly above: ~0.3 m horizontally, 5 m up.
    info.go_to_position = Some(Position { x: 0.3, y: 5.0, z: 0.0 });

    assert!(chase_target_within_reach(&info, &Position::default(), 0.5));
}

#[test]
fn chasing_far_target_does_not_hold() {
    let mut info = actor_info();
    info.go_to_position_is_chase = true;
    info.go_to_position = Some(Position { x: 3.0, y: 0.0, z: 0.0 });

    assert!(!chase_target_within_reach(&info, &Position::default(), 0.5));
}

#[test]
fn returning_actor_does_not_hold_within_reach() {
    let mut info = actor_info();
    info.go_to_position_is_chase = false; // returning to spawn
    info.go_to_position = Some(Position { x: 0.1, y: 0.0, z: 0.0 });

    // Returners must still "arrive" and advance their path, not hold.
    assert!(!chase_target_within_reach(&info, &Position::default(), 0.5));
}

#[test]
fn commit_is_reused_only_while_live_and_viable() {
    // Live window + heading still takeable → reuse (no re-decide this tick).
    assert!(should_reuse_commit(0.1, true));
    // Window lapsed → re-decide.
    assert!(!should_reuse_commit(0.0, true));
    assert!(!should_reuse_commit(-0.05, true));
    // Committed heading no longer takeable (blocked / none) → re-decide.
    assert!(!should_reuse_commit(0.1, false));
}
