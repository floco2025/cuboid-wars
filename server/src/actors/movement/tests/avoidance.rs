use super::*;
use common::{
    constants::{LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE},
    protocol::{BarrierKindTable, Ladder},
};
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

#[test]
fn blocked_climber_holds_its_rung_and_resumes_when_clear() {
    let physics = actor_physics();
    let pos = Position {
        x: 0.0,
        y: 1.0,
        z: -(LADDER_RAIL_INSET + physics.movement_collider.radius() + LADDER_STANDOFF_CLEARANCE),
    };
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            ladders: vec![Ladder {
                x1: -0.6,
                x2: 0.6,
                z1: 0.0,
                z2: 0.0,
                nx: 0.0,
                nz: -1.0,
                y: 0.0,
                height: 4.4,
                level: 0,
                levels: 1,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        &BarrierKindTable::default(),
    );
    let blockers = [(
        test_entity(2),
        Position {
            y: pos.y + physics.movement_collider.height + 0.04,
            ..pos
        },
        physics,
    )];
    let mut context = context(test_entity(1), &pos, &world, &[], &blockers);
    context.can_use_ladders = true;
    context.vertical_velocity = 2.0;
    let desired = ActorMoveIntent::Climbing {
        direction: 0.0,
        speed: 2.0,
    };
    let target = Position { y: 3.0, ..pos };
    let selected = select_route_move(&context, desired, &target);
    assert_eq!(selected.intent, desired.holding_ladder());
    assert!(selected.step.position.distance_sq(&pos) < 1e-5);
    assert_eq!(selected.step.vertical_velocity, 0.0);
    context.actor_starts = &[];
    let selected = select_route_move(&context, desired, &target);
    assert_eq!(selected.intent, desired);
    assert!(selected.step.position.y > pos.y);
}
