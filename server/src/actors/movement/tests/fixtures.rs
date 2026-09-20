pub(super) use common::protocol::CarrierId;
use std::sync::LazyLock;

pub(super) use bevy::prelude::Entity;
pub(super) use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, Position},
};

pub(super) use crate::actors::ActorInfo;

pub(super) use super::super::{
    flight::FlightMoveContext,
    ordering::{ActorPlanOrder, actor_route_distance, sort_actor_plan_order},
};

pub(crate) const TEST_KIND: &str = crate::actors::test_kinds::BEAM;
pub(crate) const TEST_DELTA: f32 = 0.1;
static NO_CARRIERS: LazyLock<Carriers> = LazyLock::new(Carriers::default);

pub(crate) fn order(entity_bits: u64, route_distance: f32, id: u32) -> ActorPlanOrder {
    ActorPlanOrder {
        entity: Entity::from_bits(entity_bits),
        route_distance,
        id: ActorId(id),
    }
}

pub(crate) fn actor_info() -> ActorInfo {
    ActorInfo::new(test_entity(1), 0, TEST_KIND.into(), CarrierId::WORLD)
}

pub(crate) fn actor_physics() -> CharacterPhysicsConfig {
    crate::actors::test_kinds::physics(TEST_KIND)
}

pub(crate) fn test_entity(index: u64) -> Entity {
    Entity::from_bits(index)
}

pub(crate) fn context<'a>(
    entity: Entity,
    pos: &'a Position,
    collision_world: &'a CollisionWorld,
    planned_moves: &'a [CharacterMovePlan],
    actor_starts: &'a [(Entity, Position, CharacterPhysicsConfig)],
) -> FlightMoveContext<'a> {
    FlightMoveContext {
        entity,
        pos,
        actor_physics: actor_physics(),
        delta: TEST_DELTA,
        collision_world,
        planned_moves,
        actor_starts,
        open_fields: &[],
        knockback_step: bevy::prelude::Vec3::ZERO,
        carriers: &NO_CARRIERS,
    }
}
