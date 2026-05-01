use bevy::prelude::*;

use common::{constants::PLAYER_HEIGHT, markers::ActorMarker, protocol::Position};

pub fn actors_transform_sync_system(mut query: Query<(&Position, &mut Transform), With<ActorMarker>>) {
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y + PLAYER_HEIGHT / 2.0;
        transform.translation.z = pos.z;
    }
}
