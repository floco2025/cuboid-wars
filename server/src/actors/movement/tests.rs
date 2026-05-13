use bevy::prelude::Entity;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::{FLOOR_THICKNESS, WALL_THICKNESS},
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, Floor, MapLayout, Position, Wall},
};
use rand::rng;

use crate::resources::{ActorAvoidanceState, ActorInfo};

use super::{
    context::{ActorMoveContext, MoveCandidateResult},
    ordering::{ActorPlanOrder, actor_target_distance_sq, sort_actor_plan_order},
    planning::{
        blocked_step_made_useful_progress, continue_wall_avoidance_if_needed, opposite_wall_avoidance_direction,
        select_go_to_actor_move, update_reached_go_to_state, wall_avoidance_directions,
    },
};

const TEST_KIND: &str = "mine_1";

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected {actual} to be near {expected}"
    );
}

fn order(entity_bits: u64, target_distance_sq: f32, id: u32) -> ActorPlanOrder {
    ActorPlanOrder {
        entity: Entity::from_bits(entity_bits),
        target_distance_sq,
        id: ActorId(id),
    }
}

fn actor_info() -> ActorInfo {
    ActorInfo {
        entity: test_entity(1),
        spawn_zone_index: 0,
        spawn_kind: TEST_KIND.into(),
        direction_timer: 0.0,
        patrol_intent: ActorMoveIntent::Idle,
        go_to_position: None,
        go_to_position_is_chase: false,
        is_returning_to_spawn: false,
        return_path: Default::default(),
        chase_reacquire_timer: 0.0,
        avoidance_state: ActorAvoidanceState::None,
    }
}

fn actor_physics() -> CharacterPhysicsConfig {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind in default gameplay config")
        .physics()
}

fn actor_speed() -> f32 {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind in default gameplay config")
        .patrol_speed
}

fn test_entity(index: u64) -> Entity {
    Entity::from_bits(index)
}

fn floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    }
}

fn wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: WALL_THICKNESS,
        level: 0,
    }
}

fn collision_world(walls: &[Wall]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: walls.to_vec(),
            floors: vec![floor()],
            ..Default::default()
        },
        &common::protocol::BarrierKindTable::default(),
    )
}

fn context<'a>(
    entity: Entity,
    pos: &'a Position,
    collision_world: &'a CollisionWorld,
    planned_moves: &'a [CharacterMovePlan],
    actor_starts: &'a [(Entity, Position)],
) -> ActorMoveContext<'a> {
    ActorMoveContext {
        entity,
        pos,
        vertical_velocity: 0.0,
        actor_physics: actor_physics(),
        delta: 0.1,
        collision_world,
        planned_moves,
        actor_starts,
        path_clear_lookahead_time: 0.4,
    }
}

fn planned_move(entity: Entity, start: Position, target: Position) -> CharacterMovePlan {
    CharacterMovePlan::from_target(entity, start, target, 0.0, actor_physics(), false)
}

#[test]
fn actor_plan_order_prioritizes_closer_go_to_target() {
    let mut order = vec![order(1, 9.0, 1), order(2, 1.0, 2), order(3, 4.0, 3)];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(3), ActorId(1),]
    );
}

#[test]
fn actor_plan_order_uses_actor_id_as_tie_breaker() {
    let mut order = vec![order(1, 4.0, 3), order(2, 4.0, 1), order(3, 4.0, 2)];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(1), ActorId(2), ActorId(3),]
    );
}

#[test]
fn actor_without_go_to_position_plans_after_targeted_actor() {
    let pos = Position::default();
    let targeted = ActorInfo {
        entity: Entity::from_bits(1),
        spawn_zone_index: 0,
        spawn_kind: TEST_KIND.into(),
        direction_timer: 0.0,
        patrol_intent: ActorMoveIntent::Idle,
        go_to_position: Some(Position { x: 1.0, y: 0.0, z: 0.0 }),
        go_to_position_is_chase: true,
        is_returning_to_spawn: false,
        return_path: Default::default(),
        chase_reacquire_timer: 0.0,
        avoidance_state: ActorAvoidanceState::None,
    };

    assert!(actor_target_distance_sq(&pos, Some(&targeted)).is_finite());
    assert_eq!(actor_target_distance_sq(&pos, None), f32::INFINITY);
}

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
    let actor_starts = [(test_entity(2), Position { x: 0.6, y: 0.0, z: 0.0 })];
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
                    x: direction.sin() * 0.6,
                    y: 0.0,
                    z: direction.cos() * 0.6,
                },
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
    let actor_starts = [(test_entity(2), Position { x: 0.6, y: 0.0, z: 0.0 })];
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
            z: 0.6,
        },
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
    let actor_starts = [(test_entity(2), front_start)];
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
