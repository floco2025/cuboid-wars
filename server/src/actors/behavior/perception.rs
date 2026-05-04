use bevy::prelude::*;

use crate::resources::PlayerMap;
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{PlayerId, PlayerMarker, Position},
};

pub(super) fn visible_player_position(
    actor_pos: &Position,
    actor_eye_height: f32,
    vision_range: f32,
    players: &PlayerMap,
    player_query: &Query<(&PlayerId, &Position), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) -> Option<Position> {
    let actor_sight_origin = Vec3::new(actor_pos.x, actor_pos.y + actor_eye_height, actor_pos.z);
    let player_physics = gameplay_config.player.physics();

    player_query
        .iter()
        .filter(|(id, _)| players.get(id).is_some_and(|info| info.logged_in))
        .filter(|(_, pos)| horizontal_distance_sq(actor_pos, pos) <= vision_range * vision_range)
        .filter(|(_, pos)| {
            let player_collider_center = Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z);
            collision_world.line_of_sight_clear(actor_sight_origin, player_collider_center)
        })
        .min_by(|(_, a), (_, b)| horizontal_distance_sq(actor_pos, a).total_cmp(&horizontal_distance_sq(actor_pos, b)))
        .map(|(_, pos)| *pos)
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}
