use bevy::prelude::*;

use super::{LocalPlayerMarker, components::CuboidShake};
use crate::characters::PreviousTickPosition;
use common::protocol::{PlayerMarker, Position};

// ============================================================================
// Transform Sync Systems
// ============================================================================

pub fn players_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    mut player_query: Query<
        (
            &Position,
            &PreviousTickPosition,
            &mut Transform,
            Option<&CuboidShake>,
            Has<LocalPlayerMarker>,
        ),
        With<PlayerMarker>,
    >,
) {
    let alpha = fixed_time.overstep_fraction();
    for (pos, prev, mut transform, maybe_shake, is_local) in &mut player_query {
        let interp = if is_local {
            prev.lerp_to(*pos, alpha)
        } else {
            Vec3::from(*pos)
        };
        transform.translation.x = interp.x;
        transform.translation.y = interp.y;
        transform.translation.z = interp.z;

        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}
