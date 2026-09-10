use bevy::prelude::*;
use common::protocol::Position;

use super::RemoteActorMotion;

pub(crate) fn actors_transform_sync_system(mut query: Query<(&Position, &mut Transform), With<RemoteActorMotion>>) {
    for (pos, mut transform) in &mut query {
        transform.translation = Vec3::from(*pos);
    }
}
