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
