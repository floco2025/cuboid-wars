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

    let desire = desired_move(&goal, &Position::default(), 4.0, 0.5);

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
        desired_move(&goal, &Position::default(), 4.0, 0.5),
        ActorDesire::Idle
    ));
}

#[test]
fn desired_move_heads_pursuit_and_return_at_their_spots() {
    let pursuit = ActorGoal::Pursuit {
        last_seen: Position { x: 3.0, y: 0.0, z: 0.0 },
    };
    assert!(matches!(
        desired_move(&pursuit, &Position::default(), 4.0, 0.5),
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
        desired_move(&returning, &Position::default(), 4.0, 0.5),
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
        desired_move(&strict, &Position::default(), 4.0, 0.5),
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
        desired_move(&escaping, &Position::default(), 4.0, 0.5),
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
        desired_move(&idle, &Position::default(), 4.0, 0.5),
        ActorDesire::Idle
    ));
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
