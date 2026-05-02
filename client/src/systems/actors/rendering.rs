use bevy::prelude::*;

use common::{config::GameplayConfig, markers::ActorMarker, protocol::Position};

pub fn actors_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    mut query: Query<(&Position, &mut Transform), With<ActorMarker>>,
) {
    let actor_height = gameplay_config.characters.actor.collider.height;
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y + actor_height / 2.0;
        transform.translation.z = pos.z;
    }
}
