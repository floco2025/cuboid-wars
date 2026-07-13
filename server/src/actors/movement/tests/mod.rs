use bevy::prelude::Entity;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::{FLOOR_THICKNESS, WALL_THICKNESS},
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, Floor, MapLayout, Position, Wall},
};

use crate::resources::{ActorGoal, ActorInfo};

use super::{
    context::{ActorMoveContext, CandidateStep, StepPolicy, blocked_step_made_useful_progress},
    ordering::{ActorPlanOrder, actor_target_distance_sq, sort_actor_plan_order},
    planning::{select_actor_move, select_committed_move, should_reuse_commit},
    steering::{ActorDesire, desired_move},
};

mod avoidance;
mod ordering;
mod state;

const TEST_KIND: &str = "mine_1";
const TEST_DELTA: f32 = 0.1;

fn order(entity_bits: u64, target_distance_sq: f32, id: u32) -> ActorPlanOrder {
    ActorPlanOrder {
        entity: Entity::from_bits(entity_bits),
        target_distance_sq,
        id: ActorId(id),
    }
}

fn actor_info() -> ActorInfo {
    ActorInfo::new(
        test_entity(1),
        0,
        TEST_KIND.into(),
        ActorGoal::Patrol {
            intent: ActorMoveIntent::Idle,
            direction_timer: 0.0,
            ledge_escape_timer: 0.0,
        },
    )
}

fn actor_physics() -> CharacterPhysicsConfig {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind missing from default gameplay config")
        .physics()
}

fn actor_speed() -> f32 {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind missing from default gameplay config")
        .patrol_speed
}

fn actor_blocker_distance() -> f32 {
    let physics = actor_physics();
    physics.collider.width.max(physics.collider.depth) + actor_speed() * TEST_DELTA * 0.75
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
    actor_starts: &'a [(Entity, Position, CharacterPhysicsConfig)],
) -> ActorMoveContext<'a> {
    ActorMoveContext {
        entity,
        pos,
        vertical_velocity: 0.0,
        actor_physics: actor_physics(),
        delta: TEST_DELTA,
        collision_world,
        planned_moves,
        actor_starts,
        path_clear_lookahead_secs: 0.4,
        open_barrier_kinds: &[],
        gravity: 25.0,
        knockback_step: bevy::prelude::Vec3::ZERO,
    }
}

fn planned_move(entity: Entity, start: Position, target: Position) -> CharacterMovePlan {
    CharacterMovePlan::from_target(entity, start, target, 0.0, actor_physics(), false)
}
