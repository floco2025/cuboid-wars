use bevy::prelude::Entity;
use common::protocol::{ActorId, Position};

use crate::actors::{ActorInfo, ActorMap};

use super::query::ActorMovementQuery;

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct ActorPlanOrder {
    pub(super) entity: Entity,
    pub(super) route_distance: f32,
    pub(super) id: ActorId,
}

pub(super) fn sorted_actor_plan_order(query: &ActorMovementQuery, actors: &ActorMap) -> Vec<ActorPlanOrder> {
    let mut order: Vec<ActorPlanOrder> = query
        .iter()
        .map(|(entity, id, _, pos, _, _, _, _)| ActorPlanOrder {
            entity,
            route_distance: actor_route_distance(pos, actors.get(id)),
            id: *id,
        })
        .collect();
    sort_actor_plan_order(&mut order);
    order
}

pub(super) fn sort_actor_plan_order(order: &mut [ActorPlanOrder]) {
    order.sort_by(|a, b| {
        a.route_distance
            .total_cmp(&b.route_distance)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
}

pub(super) fn actor_route_distance(pos: &Position, info: Option<&ActorInfo>) -> f32 {
    let Some(route) = info
        .and_then(|info| info.route.as_ref())
        .filter(|route| !route.waypoints.is_empty())
    else {
        return f32::INFINITY;
    };
    let mut distance = 0.0;
    let mut previous = *pos;
    for waypoint in &route.waypoints {
        distance += previous.horizontal_distance_sq(waypoint).sqrt();
        previous = *waypoint;
    }
    distance
}
