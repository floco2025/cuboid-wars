use bevy::prelude::*;

use crate::{actors::ActorMap, characters::PreviousTickPosition};
use common::{
    config::GameplayConfig,
    protocol::{ActorId, ActorMarker, Position},
};

// Interpolate actor `Transform` between the last-tick and current-tick
// `Position` using the fixed-step overstep fraction. See the player
// equivalent for context.
pub fn actors_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    actors: Res<ActorMap>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&ActorId, &Position, &PreviousTickPosition, &mut Transform), With<ActorMarker>>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (id, pos, prev, mut transform) in &mut query {
        let Some(info) = actors.get(id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is missing from gameplay config")
            .physics();
        let interp = prev.lerp_to(*pos, alpha);
        transform.translation.x = interp.x;
        transform.translation.y = actor_physics.collider_center_y(interp.y);
        transform.translation.z = interp.z;
    }
}
