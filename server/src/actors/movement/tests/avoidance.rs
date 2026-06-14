use super::*;
use std::f32::consts::{FRAC_PI_2, TAU};

// ============================================================================
// Per-candidate evaluation primitives (design-independent — kept across the
// avoidance rework).
// ============================================================================

#[test]
fn blocked_step_needs_meaningful_progress() {
    let start = Position::default();
    let too_short = Position {
        x: 0.24,
        y: 0.0,
        z: 0.0,
    };
    let useful = Position {
        x: 0.26,
        y: 0.0,
        z: 0.0,
    };

    assert!(!blocked_step_made_useful_progress(&start, &too_short, 5.0, 0.1));
    assert!(blocked_step_made_useful_progress(&start, &useful, 5.0, 0.1));
}

#[test]
fn candidate_blocked_by_static_world_is_not_character_blocked() {
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let collision_world = collision_world(&[wall()]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: 20.0,
    };

    match context.evaluate_candidate(intent) {
        MoveCandidateResult::BlockedByWorld { selected } => {
            assert!(selected.step.blocked);
            assert_eq!(selected.intent, intent);
        }
        MoveCandidateResult::Accepted { .. } | MoveCandidateResult::BlockedByCharacter => {
            panic!("expected static-world block")
        }
    }
}

#[test]
fn candidate_blocked_by_other_character() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [(
        test_entity(2),
        Position {
            x: actor_blocker_distance(),
            y: 0.0,
            z: 0.0,
        },
        actor_physics(),
    )];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_candidate(intent) {
        MoveCandidateResult::BlockedByCharacter => {}
        MoveCandidateResult::Accepted { .. } | MoveCandidateResult::BlockedByWorld { .. } => {
            panic!("expected character block")
        }
    }
}

#[test]
fn following_front_actor_is_not_blocked_when_final_positions_do_not_overlap() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let front_start = Position { x: 1.2, y: 0.0, z: 0.0 };
    let planned_moves = [planned_move(
        test_entity(2),
        front_start,
        Position { x: 2.0, y: 0.0, z: 0.0 },
    )];
    let actor_starts = [(test_entity(2), front_start, actor_physics())];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_candidate(intent) {
        MoveCandidateResult::Accepted { .. } => {}
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            panic!("expected following move to be accepted")
        }
    }
}

// The shared `floor()` helper is an 8m square centered at the origin
// (x,z in [-4, 4]) at y=0. With patrol speed and the default lookahead time, a
// step from x=3.5 moving east projects past x=4. The single-tick step stays on
// the floor; the lookahead probe catches the impending fall.
#[test]
fn patrol_candidate_rejected_when_lookahead_lands_off_floor() {
    let pos = Position { x: 3.5, y: 0.0, z: 0.0 };
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_patrol_candidate(intent) {
        MoveCandidateResult::BlockedByWorld { .. } => {}
        MoveCandidateResult::Accepted { .. } | MoveCandidateResult::BlockedByCharacter => {
            panic!("expected patrol candidate to be blocked by ledge ahead")
        }
    }
}

#[test]
fn patrol_candidate_accepted_when_lookahead_stays_on_floor() {
    let pos = Position { x: 3.5, y: 0.0, z: 0.0 };
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: -FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_patrol_candidate(intent) {
        MoveCandidateResult::Accepted { .. } => {}
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            panic!("expected patrol candidate moving inward to be accepted")
        }
    }
}

// ============================================================================
// The unified direction-scoring selector.
// ============================================================================

#[test]
fn select_takes_clear_path_straight() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let context = context(test_entity(1), &pos, &collision_world, &[], &[]);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    let selected = select_actor_move(&context, desired, None, false);

    assert_eq!(
        selected.intent, desired,
        "an unobstructed actor heads straight for the goal"
    );
}

#[test]
fn select_turns_around_a_wall_instead_of_freezing() {
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let collision_world = collision_world(&[wall()]);
    let context = context(test_entity(1), &pos, &collision_world, &[], &[]);
    // High speed so the single step actually reaches the wall at x=0.
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: 20.0,
    };

    let selected = select_actor_move(&context, desired, None, false);

    assert_ne!(
        selected.intent,
        ActorMoveIntent::Idle,
        "should route around the wall, not freeze"
    );
    assert_ne!(
        selected.intent.direction(),
        Some(FRAC_PI_2),
        "should not pick the wall-blocked straight heading"
    );
}

// The bug this rework fixes: an actor blocked head-on by another character used
// to have no backward escape and could freeze. The full-circle selector must
// always find a free heading when one exists.
#[test]
fn select_escapes_head_on_character_block() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let blocker = [(
        test_entity(2),
        Position {
            x: actor_blocker_distance(),
            y: 0.0,
            z: 0.0,
        },
        actor_physics(),
    )];
    let context = context(test_entity(1), &pos, &collision_world, &[], &blocker);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2, // straight at the blocker
        speed: actor_speed(),
    };

    let selected = select_actor_move(&context, desired, None, false);

    assert_ne!(
        selected.intent,
        ActorMoveIntent::Idle,
        "a single blocker in the open must never cause a freeze"
    );
}

#[test]
fn two_actors_facing_each_other_do_not_both_freeze() {
    let collision_world = collision_world(&[]);
    let a_pos = Position::default();
    let b_pos = Position {
        x: actor_blocker_distance(),
        y: 0.0,
        z: 0.0,
    };

    // A plans with B as a stationary blocker dead ahead.
    let a_starts = [(test_entity(2), b_pos, actor_physics())];
    let a_context = context(test_entity(1), &a_pos, &collision_world, &[], &a_starts);
    let a_move = select_actor_move(
        &a_context,
        ActorMoveIntent::Moving {
            direction: FRAC_PI_2,
            speed: actor_speed(),
        },
        None,
        false,
    );
    // B plans facing the opposite way with A as a stationary blocker dead ahead.
    let b_starts = [(test_entity(1), a_pos, actor_physics())];
    let b_context = context(test_entity(2), &b_pos, &collision_world, &[], &b_starts);
    let b_move = select_actor_move(
        &b_context,
        ActorMoveIntent::Moving {
            direction: -FRAC_PI_2,
            speed: actor_speed(),
        },
        None,
        false,
    );

    assert_ne!(a_move.intent, ActorMoveIntent::Idle);
    assert_ne!(b_move.intent, ActorMoveIntent::Idle);
}

#[test]
fn select_idles_only_when_fully_ringed() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let distance = actor_blocker_distance();
    // A blocker dead-ahead of every candidate heading (12 around the circle).
    let starts: Vec<_> = (0..12)
        .map(|i| {
            let angle = i as f32 * TAU / 12.0;
            (
                test_entity(i + 2),
                Position {
                    x: angle.sin() * distance,
                    y: 0.0,
                    z: angle.cos() * distance,
                },
                actor_physics(),
            )
        })
        .collect();
    let context = context(test_entity(1), &pos, &collision_world, &[], &starts);
    let desired = ActorMoveIntent::Moving {
        direction: 0.0,
        speed: actor_speed(),
    };

    let selected = select_actor_move(&context, desired, None, false);

    assert_eq!(selected.intent, ActorMoveIntent::Idle, "no free heading exists → idle");
}

#[test]
fn patrol_select_does_not_step_off_a_ledge() {
    let pos = Position { x: 3.5, y: 0.0, z: 0.0 };
    let collision_world = collision_world(&[]);
    let context = context(test_entity(1), &pos, &collision_world, &[], &[]);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2, // straight at the x=4 edge
        speed: actor_speed(),
    };

    let selected = select_actor_move(&context, desired, None, true); // ledge-aware (patrol)

    assert_ne!(
        selected.intent,
        ActorMoveIntent::Idle,
        "inward headings stay on the floor"
    );
    assert_ne!(
        selected.intent.direction(),
        Some(FRAC_PI_2),
        "patrol must not walk off the edge"
    );
}

// ============================================================================
// Steering commit (hold a chosen heading for a short window).
// ============================================================================

#[test]
fn commit_reuses_committed_heading_at_current_speed() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let context = context(test_entity(1), &pos, &collision_world, &[], &[]);
    let mut info = actor_info();

    // First decision (open field) commits a heading toward +x at speed 4.
    let first = select_committed_move(
        &mut info,
        &context,
        ActorMoveIntent::Moving {
            direction: FRAC_PI_2,
            speed: 4.0,
        },
        None,
        false,
        0.05,
    );
    assert_eq!(
        first.intent,
        ActorMoveIntent::Moving {
            direction: FRAC_PI_2,
            speed: 4.0
        }
    );
    assert_eq!(info.committed_direction, Some(FRAC_PI_2));

    // Within the window, a desire with a different direction AND a slower speed
    // reuses the committed *heading* but at the *current* speed — the fix for a
    // chase commit carrying chase speed into patrol.
    let reused = select_committed_move(
        &mut info,
        &context,
        ActorMoveIntent::Moving {
            direction: 0.3,
            speed: 2.0,
        },
        None,
        true,
        0.05,
    );
    assert_eq!(
        reused.intent,
        ActorMoveIntent::Moving {
            direction: FRAC_PI_2,
            speed: 2.0
        }
    );
}

#[test]
fn commit_redecides_after_window_expires() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let context = context(test_entity(1), &pos, &collision_world, &[], &[]);
    let mut info = actor_info();

    select_committed_move(
        &mut info,
        &context,
        ActorMoveIntent::Moving {
            direction: FRAC_PI_2,
            speed: 4.0,
        },
        None,
        false,
        0.05,
    );
    assert_eq!(info.committed_direction, Some(FRAC_PI_2));

    // A delta past the commit window forces a fresh decision toward the new desire.
    let after = select_committed_move(
        &mut info,
        &context,
        ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 4.0,
        },
        None,
        false,
        1.0,
    );
    assert_eq!(
        after.intent,
        ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 4.0
        }
    );
    assert_eq!(info.committed_direction, Some(0.0));
}
