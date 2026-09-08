use bevy::prelude::*;

use crate::{actors::ActorMap, characters::PreviousTickPosition};
use common::{
    map::Carriers,
    protocol::{ActorId, ActorMarker, Position},
};

// Interpolate actor `Transform` between the last-tick and current-tick
// `Position` using the fixed-step overstep fraction. See the player
// equivalent for context.
pub fn actors_transform_sync_system(
    actors: Res<ActorMap>,
    carriers: Res<Carriers>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&ActorId, &Position, &PreviousTickPosition, &mut Transform), With<ActorMarker>>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (id, pos, prev, mut transform) in &mut query {
        let Some(info) = actors.get(id) else {
            continue;
        };
        let interp = info.anchor.map_or_else(
            || prev.lerp_to(*pos, alpha),
            |anchor| {
                carriers
                    .pose_between(anchor.carrier, alpha)
                    .transform_point(Vec3::from(anchor.pos))
            },
        );
        transform.translation.x = interp.x;
        transform.translation.y = interp.y;
        transform.translation.z = interp.z;
    }
}
