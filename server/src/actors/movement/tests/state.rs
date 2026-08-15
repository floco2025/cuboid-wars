use super::*;

fn chase(target: Position) -> ActorGoal {
    ActorGoal::Chase { target }
}

#[test]
fn chase_holds_when_target_is_above() {
    // Player on a ledge directly above: ~0.3 m horizontally, 5 m up.
    let goal = chase(Position { x: 0.3, y: 5.0, z: 0.0 });
    assert!(goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn chase_holds_when_target_is_below() {
    // Player tucked below an overhang: without the abs() the actor would
    // orbit-jitter around their overhead point.
    let goal = chase(Position {
        x: 0.3,
        y: -5.0,
        z: 0.0,
    });
    assert!(goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn chase_does_not_hold_at_same_height() {
    // Within reach horizontally but at the same height (e.g. through a thin
    // wall): the chase must keep pressing so the stall watchdog can end it.
    let goal = chase(Position { x: 0.3, y: 0.0, z: 0.0 });
    assert!(!goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn chase_does_not_hold_below_jump_height() {
    // A hopping same-height player must not flicker the hold on and off.
    let goal = chase(Position { x: 0.3, y: 0.5, z: 0.0 });
    assert!(!goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn chase_does_not_hold_on_far_target() {
    let goal = chase(Position { x: 3.0, y: 5.0, z: 0.0 });
    assert!(!goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn only_chases_hold() {
    // A returner right on top of its waypoint must still arrive, not hold.
    let goal = ActorGoal::Return {
        next: Position { x: 0.1, y: 5.0, z: 0.0 },
        path: Default::default(),
    };
    assert!(!goal.chase_hold(&Position::default(), 0.5));
}

#[test]
fn desired_move_presses_chase_within_reach() {
    // Same height within reach: pressing, never "arriving".
    let goal = chase(Position { x: 0.3, y: 0.0, z: 0.0 });

    let desire = desired_move(&goal, &Position::default(), 4.0, 0.5, None);

    assert!(matches!(
        desire,
        ActorDesire::Move {
            intent: ActorMoveIntent::Moving { speed, .. },
            policy: StepPolicy::Pursue,
        } if speed == 4.0
    ));
}

#[test]
fn desired_move_idles_a_holding_chase() {
    let goal = chase(Position { x: 0.3, y: 5.0, z: 0.0 });
    assert!(matches!(
        desired_move(&goal, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Idle
    ));
}

#[test]
fn desired_move_heads_pursuit_and_return_at_their_spots() {
    let pursuit = ActorGoal::Pursuit {
        last_seen: Position { x: 3.0, y: 0.0, z: 0.0 },
    };
    assert!(matches!(
        desired_move(&pursuit, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Move {
            policy: StepPolicy::Pursue,
            ..
        }
    ));

    let returning = ActorGoal::Return {
        next: Position { x: 3.0, y: 0.0, z: 0.0 },
        path: Default::default(),
    };
    assert!(matches!(
        desired_move(&returning, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Move {
            policy: StepPolicy::Pursue,
            ..
        }
    ));
}

#[test]
fn desired_move_patrol_policy_follows_escape_window() {
    let intent = ActorMoveIntent::Moving {
        direction: 0.0,
        speed: 2.0,
    };

    let strict = ActorGoal::Patrol {
        intent,
        direction_timer: 1.0,
        ledge_escape_timer: 0.0,
    };
    assert!(matches!(
        desired_move(&strict, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Move {
            policy: StepPolicy::Strict,
            ..
        }
    ));

    let escaping = ActorGoal::Patrol {
        intent,
        direction_timer: 1.0,
        ledge_escape_timer: 0.5,
    };
    assert!(matches!(
        desired_move(&escaping, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Move {
            policy: StepPolicy::Pursue,
            ..
        }
    ));

    let idle = ActorGoal::Patrol {
        intent: ActorMoveIntent::Idle,
        direction_timer: 1.0,
        ledge_escape_timer: 0.0,
    };
    assert!(matches!(
        desired_move(&idle, &Position::default(), 4.0, 0.5, None),
        ActorDesire::Idle
    ));
}

fn fire_config() -> crate::config::ActorFireConfig {
    crate::config::ActorFireConfig {
        standoff_distance: 6.0,
        range: 15.0,
        duration_secs: 1.0,
        cooldown_secs: 5.0,
        damage_per_second: 75.0,
    }
}

#[test]
fn approach_holds_at_standoff() {
    let fire = fire_config();
    let goal = ActorGoal::Approach {
        target: common::protocol::PlayerId(7),
        target_pos: Position { x: 4.0, y: 0.0, z: 0.0 },
    };
    assert!(matches!(
        desired_move(&goal, &Position::default(), 4.0, 0.5, Some(&fire)),
        ActorDesire::Idle
    ));

    // Outside the standoff the approach presses like a chase.
    let far = ActorGoal::Approach {
        target: common::protocol::PlayerId(7),
        target_pos: Position {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        },
    };
    assert!(matches!(
        desired_move(&far, &Position::default(), 4.0, 0.5, Some(&fire)),
        ActorDesire::Move {
            policy: StepPolicy::Pursue,
            ..
        }
    ));
}

#[test]
fn fire_holds_and_faces_target() {
    let fire = fire_config();
    let target_pos = Position { x: 0.0, y: 0.0, z: 5.0 };
    let goal = ActorGoal::Fire {
        target: common::protocol::PlayerId(7),
        target_pos,
        remaining_secs: 0.5,
    };
    let ActorDesire::HoldFacing { direction } = desired_move(&goal, &Position::default(), 4.0, 0.5, Some(&fire)) else {
        panic!("a firing actor must hold and face its target");
    };
    assert!((direction - direction_toward(&Position::default(), &target_pos)).abs() < 1e-6);
}

#[test]
fn flee_direction_points_away_from_threat() {
    let fire = fire_config();
    let threat = Position { x: 5.0, y: 0.0, z: 0.0 };
    let goal = ActorGoal::Flee { threat };
    let ActorDesire::Move {
        intent: ActorMoveIntent::Moving { direction, speed },
        policy: StepPolicy::Strict,
    } = desired_move(&goal, &Position::default(), 4.0, 0.5, Some(&fire))
    else {
        panic!("a fleeing actor must move strictly");
    };
    assert_eq!(speed, 4.0);
    assert!((direction - direction_toward(&threat, &Position::default())).abs() < 1e-6);
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
