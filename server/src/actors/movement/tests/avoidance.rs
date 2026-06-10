use super::*;
use rand::rng;

#[test]
fn wall_avoidance_directions_try_one_side_then_the_other() {
    let directions = wall_avoidance_directions(1.0, 1.0);

    assert_near(directions[0], 1.0 + std::f32::consts::FRAC_PI_2);
    assert_near(directions[1], 1.0 - std::f32::consts::FRAC_PI_2);
}

#[test]
fn wall_avoidance_directions_respect_randomized_side_order() {
    let directions = wall_avoidance_directions(1.0, -1.0);

    assert_near(directions[0], 1.0 - std::f32::consts::FRAC_PI_2);
    assert_near(directions[1], 1.0 + std::f32::consts::FRAC_PI_2);
}

#[test]
fn opposite_wall_avoidance_direction_turns_around() {
    assert_near(opposite_wall_avoidance_direction(1.0), 1.0 + std::f32::consts::PI);
}

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
        direction: std::f32::consts::FRAC_PI_2,
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
fn candidate_blocked_by_other_character_yields_before_wall_avoidance() {
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
        direction: std::f32::consts::FRAC_PI_2,
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
fn character_blocked_steering_selects_idle_when_all_steering_options_are_blocked() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let base_direction = std::f32::consts::FRAC_PI_2;
    let blocked_positions = [
        base_direction,
        base_direction + 20.0_f32.to_radians(),
        base_direction + 45.0_f32.to_radians(),
        base_direction + 90.0_f32.to_radians(),
        base_direction + 135.0_f32.to_radians(),
        base_direction - 20.0_f32.to_radians(),
        base_direction - 45.0_f32.to_radians(),
        base_direction - 90.0_f32.to_radians(),
        base_direction - 135.0_f32.to_radians(),
    ];
    let actor_starts: Vec<_> = blocked_positions
        .into_iter()
        .enumerate()
        .map(|(index, direction)| {
            (
                test_entity(index as u64 + 2),
                Position {
                    x: direction.sin() * actor_blocker_distance(),
                    y: 0.0,
                    z: direction.cos() * actor_blocker_distance(),
                },
                actor_physics(),
            )
        })
        .collect();
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let mut info = actor_info();
    let mut rng = rng();

    let selected = select_go_to_actor_move(
        &context,
        ActorMoveIntent::Moving {
            direction: base_direction,
            speed: actor_speed(),
        },
        None,
        &mut info,
        &mut rng,
    );

    assert_eq!(selected.intent, ActorMoveIntent::Idle);
    assert_eq!(info.avoidance_state, ActorAvoidanceState::None);
}

#[test]
fn character_blocked_steering_tries_sidestep_before_idling() {
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
    let mut info = actor_info();
    let mut rng = rng();

    let selected = select_go_to_actor_move(
        &context,
        ActorMoveIntent::Moving {
            direction: std::f32::consts::FRAC_PI_2,
            speed: actor_speed(),
        },
        None,
        &mut info,
        &mut rng,
    );

    assert_ne!(selected.intent, ActorMoveIntent::Idle);
    assert!(matches!(info.avoidance_state, ActorAvoidanceState::Character { .. }));
}

#[test]
fn wall_avoidance_clears_when_direct_path_is_clear_again() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let mut info = actor_info();
    info.avoidance_state = ActorAvoidanceState::Wall {
        direction: std::f32::consts::FRAC_PI_2,
    };

    let selected = continue_wall_avoidance_if_needed(
        &context,
        0.0,
        actor_speed(),
        Some(Position { x: 0.0, y: 0.0, z: 5.0 }),
        &mut info,
    );

    assert!(selected.is_none());
    assert_eq!(info.avoidance_state, ActorAvoidanceState::None);
}

#[test]
fn character_avoidance_does_not_continue_as_wall_avoidance() {
    let pos = Position::default();
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let mut info = actor_info();
    info.avoidance_state = ActorAvoidanceState::Character { direction: 0.0 };

    let selected = continue_wall_avoidance_if_needed(
        &context,
        std::f32::consts::FRAC_PI_2,
        actor_speed(),
        Some(Position { x: 5.0, y: 0.0, z: 0.0 }),
        &mut info,
    );

    assert!(selected.is_none());
    assert_eq!(info.avoidance_state, ActorAvoidanceState::None);
}

#[test]
fn committed_wall_avoidance_yields_when_blocked_by_character() {
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let collision_world = collision_world(&[wall()]);
    let planned_moves = [];
    let actor_starts = [(
        test_entity(2),
        Position {
            x: -1.0,
            y: 0.0,
            z: actor_blocker_distance(),
        },
        actor_physics(),
    )];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let mut info = actor_info();
    info.avoidance_state = ActorAvoidanceState::Wall { direction: 0.0 };

    let selected = continue_wall_avoidance_if_needed(
        &context,
        std::f32::consts::FRAC_PI_2,
        actor_speed(),
        Some(Position { x: 5.0, y: 0.0, z: 0.0 }),
        &mut info,
    )
    .expect("wall avoidance should yield with an idle move");

    assert_eq!(selected.intent, ActorMoveIntent::Idle);
    assert_eq!(info.avoidance_state, ActorAvoidanceState::Wall { direction: 0.0 });
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
        direction: std::f32::consts::FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_candidate(intent) {
        MoveCandidateResult::Accepted { .. } => {}
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            panic!("expected following move to be accepted")
        }
    }
}

// The shared `floor()` helper above is an 8m square centered at the origin
// (x in [-4, 4], z in [-4, 4]) at y=0. With actor patrol speed and the default
// lookahead time, a step from x=3.5 moving east projects past x=4. The
// single-tick step only moves the actor 0.3m, so it stays on the floor for
// that step; the lookahead probe catches the impending fall.
#[test]
fn patrol_candidate_rejected_when_lookahead_lands_off_floor() {
    let pos = Position { x: 3.5, y: 0.0, z: 0.0 };
    let collision_world = collision_world(&[]);
    let planned_moves = [];
    let actor_starts = [];
    let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
    let intent = ActorMoveIntent::Moving {
        direction: std::f32::consts::FRAC_PI_2,
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
        direction: -std::f32::consts::FRAC_PI_2,
        speed: actor_speed(),
    };

    match context.evaluate_patrol_candidate(intent) {
        MoveCandidateResult::Accepted { .. } => {}
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            panic!("expected patrol candidate moving inward to be accepted")
        }
    }
}
