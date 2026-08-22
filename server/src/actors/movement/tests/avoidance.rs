use super::*;
use std::f32::consts::FRAC_PI_2;

#[test]
fn blocked_step_needs_meaningful_progress() {
    let start = Position::default();
    let too_short = Position {
        x: 0.19,
        ..Position::default()
    };
    let useful = Position {
        x: 0.21,
        ..Position::default()
    };

    assert!(!blocked_step_made_useful_progress(&start, &too_short, 4.0, 0.1));
    assert!(blocked_step_made_useful_progress(&start, &useful, 4.0, 0.1));
}

#[test]
fn route_move_heads_directly_to_unobstructed_waypoint() {
    let pos = Position::default();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    let world = collision_world(&[]);
    let context = context(test_entity(1), &pos, &world, &[], &[]);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    let selected = select_route_move(&context, desired, &target);

    assert_eq!(selected.intent, desired);
}

#[test]
fn route_move_waits_at_static_wall_instead_of_inventing_a_heading() {
    let pos = Position {
        x: -1.0,
        ..Position::default()
    };
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    let world = collision_world(&[wall()]);
    let context = context(test_entity(1), &pos, &world, &[], &[]);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: 20.0,
    };

    let selected = select_route_move(&context, desired, &target);

    assert_eq!(selected.intent, ActorMoveIntent::Idle);
}

#[test]
fn route_move_uses_a_stable_sidestep_for_a_character_blocker() {
    let pos = Position::default();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    let world = collision_world(&[]);
    let blockers = [(
        test_entity(2),
        Position {
            x: actor_blocker_distance(),
            ..Position::default()
        },
        actor_physics(),
    )];
    let context = context(test_entity(1), &pos, &world, &[], &blockers);
    let desired = ActorMoveIntent::Moving {
        direction: FRAC_PI_2,
        speed: actor_speed(),
    };

    let selected = select_route_move(&context, desired, &target);

    assert_eq!(selected.intent.direction(), Some(FRAC_PI_2 + FRAC_PI_2));
}
