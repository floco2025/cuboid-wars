use bevy::prelude::*;

use common::{config::GameplayConfig, markers::ActorMarker, protocol::Position};

pub fn actors_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    mut query: Query<(&Position, &mut Transform), With<ActorMarker>>,
) {
    let actor_physics = gameplay_config.characters.actor.physics();
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.x;
        transform.translation.y = actor_physics.collider_center_y(pos.y);
        transform.translation.z = pos.z;
    }
}
