use bevy::prelude::Entity;
use common::protocol::{ActorId, Position};

use crate::resources::{ActorInfo, ActorMap};

use super::{context::horizontal_distance_sq, query::ActorMovementQuery};

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct ActorPlanOrder {
    pub(super) entity: Entity,
    pub(super) target_distance_sq: f32,
    pub(super) id: ActorId,
}

pub(super) fn sorted_actor_plan_order(query: &ActorMovementQuery, actors: &ActorMap) -> Vec<ActorPlanOrder> {
    let mut order: Vec<ActorPlanOrder> = query
        .iter()
        .map(|(entity, id, pos, _, _, _)| ActorPlanOrder {
            entity,
            target_distance_sq: actor_target_distance_sq(pos, actors.get(id)),
            id: *id,
        })
        .collect();
    sort_actor_plan_order(&mut order);
    order
}

pub(super) fn sort_actor_plan_order(order: &mut [ActorPlanOrder]) {
    order.sort_by(|a, b| {
        a.target_distance_sq
            .total_cmp(&b.target_distance_sq)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
}

pub(super) fn actor_target_distance_sq(pos: &Position, info: Option<&ActorInfo>) -> f32 {
    info.and_then(|info| info.go_to_position)
        .map_or(f32::INFINITY, |target| horizontal_distance_sq(pos, &target))
}
