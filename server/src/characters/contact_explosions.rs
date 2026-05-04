use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::CharacterMovePlan,
    protocol::{ActorMarker, Health, Position},
};

use crate::{config::ServerGameplayConfig, resources::ActorMap};

pub(super) fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
    server_gameplay_config: &ServerGameplayConfig,
) {
    for planned_move in planned_moves {
        if actors.values().any(|actor| actor.entity == planned_move.entity) {
            continue;
        }

        for actor_entity in planned_moves
            .iter()
            .filter(|other| {
                let Some(actor_info) = actors.values().find(|actor| actor.entity == other.entity) else {
                    return false;
                };
                let contact_explosion_distance = server_gameplay_config
                    .validated_actor(&actor_info.spawn_kind)
                    .contact_explosion_distance;
                character_move_plans_touch(planned_move, other, contact_explosion_distance)
            })
            .map(|actor_move| actor_move.entity)
        {
            if let Ok(mut health) = actor_health.get_mut(actor_entity) {
                health.0 = 0.0;
            }
        }
    }
}

fn character_move_plans_touch(a: &CharacterMovePlan, b: &CharacterMovePlan, contact_explosion_distance: f32) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    vertical_ranges_overlap(a.target, a.physics, b.target, b.physics)
        && horizontal_distance_sq(&a.target, &b.target)
            <= contact_distance(a.physics, b.physics, contact_explosion_distance).powi(2)
}

fn contact_distance(a: CharacterPhysicsConfig, b: CharacterPhysicsConfig, contact_explosion_distance: f32) -> f32 {
    horizontal_collider_radius(a) + horizontal_collider_radius(b) + contact_explosion_distance
}

fn horizontal_collider_radius(physics: CharacterPhysicsConfig) -> f32 {
    physics.collider.width.max(physics.collider.depth) / 2.0
}

fn vertical_ranges_overlap(
    a_pos: Position,
    a_physics: CharacterPhysicsConfig,
    b_pos: Position,
    b_physics: CharacterPhysicsConfig,
) -> bool {
    let a_bottom = a_pos.y + a_physics.collider.bottom_y_offset();
    let a_top = a_pos.y + a_physics.collider.top_y_offset();
    let b_bottom = b_pos.y + b_physics.collider.bottom_y_offset();
    let b_top = b_pos.y + b_physics.collider.top_y_offset();
    a_bottom <= b_top && b_bottom <= a_top
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}
