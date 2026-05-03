use bevy::prelude::*;

use crate::resources::ActorMap;
use common::{
    config::GameplayConfig,
    markers::ActorMarker,
    protocol::{ActorId, Position},
};

pub fn actors_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    actors: Res<ActorMap>,
    mut query: Query<(&ActorId, &Position, &mut Transform), With<ActorMarker>>,
) {
    for (id, pos, mut transform) in &mut query {
        let Some(info) = actors.0.get(id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is in gameplay config")
            .physics();
        transform.translation.x = pos.x;
        transform.translation.y = actor_physics.collider_center_y(pos.y);
        transform.translation.z = pos.z;
    }
}
