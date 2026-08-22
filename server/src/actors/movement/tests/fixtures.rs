pub(super) use bevy::prelude::Entity;
pub(super) use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::{FLOOR_THICKNESS, WALL_THICKNESS},
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, Floor, MapLayout, Position, Wall},
};

pub(super) use crate::actors::{ActorInfo, ActorMode, ActorRoute, BeamState, navigation::NavNode};

pub(super) use super::super::{
    context::{ActorMoveContext, blocked_step_made_useful_progress},
    ordering::{ActorPlanOrder, actor_route_distance, sort_actor_plan_order},
    planning::select_route_move,
    steering::{ActorDesire, desired_move, direction_toward},
};

pub(crate) const TEST_KIND: &str = "zapper";
pub(crate) const TEST_DELTA: f32 = 0.1;

pub(crate) fn order(entity_bits: u64, route_distance: f32, id: u32) -> ActorPlanOrder {
    ActorPlanOrder {
        entity: Entity::from_bits(entity_bits),
        route_distance,
        id: ActorId(id),
    }
}

pub(crate) fn actor_info() -> ActorInfo {
    ActorInfo::new(test_entity(1), 0, TEST_KIND.into())
}

pub(crate) fn route(target: Position) -> ActorRoute {
    ActorRoute {
        waypoints: [target].into(),
        destination: target,
        destination_node: NavNode {
            level: 0,
            row: 0,
            col: 0,
        },
    }
}

pub(crate) fn actor_physics() -> CharacterPhysicsConfig {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind missing from default gameplay config")
        .physics()
}

pub(crate) fn actor_speed() -> f32 {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .actor(TEST_KIND)
        .expect("test kind missing from default gameplay config")
        .roam_speed
}

pub(crate) fn actor_blocker_distance() -> f32 {
    let physics = actor_physics();
    physics.collider.width.max(physics.collider.depth) + actor_speed() * TEST_DELTA * 0.75
}

pub(crate) fn test_entity(index: u64) -> Entity {
    Entity::from_bits(index)
}

pub(crate) fn floor() -> Floor {
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

pub(crate) fn wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: WALL_THICKNESS,
        level: 0,
    }
}

pub(crate) fn collision_world(walls: &[Wall]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: walls.to_vec(),
            floors: vec![floor()],
            ..Default::default()
        },
        &common::protocol::BarrierKindTable::default(),
    )
}

pub(crate) fn context<'a>(
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
        open_barrier_kinds: &[],
        gravity: 25.0,
        ladders: test_ladders(),
        knockback_step: bevy::prelude::Vec3::ZERO,
    }
}

pub(crate) fn test_ladders() -> common::config::LaddersConfig {
    common::config::GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .ladders
}
