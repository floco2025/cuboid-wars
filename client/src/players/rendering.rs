use bevy::prelude::*;

use super::components::CuboidShake;
use crate::characters::PreviousTickPosition;
use common::{
    config::GameplayConfig,
    protocol::{PlayerMarker, Position},
};

// ============================================================================
// Transform Sync Systems
// ============================================================================

// Update player `Transform` from `Position` for rendering. Physics ticks at
// a fixed 30 Hz while rendering runs at the display rate; interpolate
// between the last-tick and current-tick positions using the fixed-step
// overstep fraction so motion stays smooth.
pub fn players_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    fixed_time: Res<Time<Fixed>>,
    mut player_query: Query<
        (&Position, &PreviousTickPosition, &mut Transform, Option<&CuboidShake>),
        With<PlayerMarker>,
    >,
) {
    let player_physics = gameplay_config.player.physics();
    let alpha = fixed_time.overstep_fraction();
    for (pos, prev, mut transform, maybe_shake) in &mut player_query {
        let interp = prev.lerp_to(*pos, alpha);
        transform.translation.x = interp.x;
        transform.translation.y = player_physics.collider_center_y(interp.y);
        transform.translation.z = interp.z;

        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}
