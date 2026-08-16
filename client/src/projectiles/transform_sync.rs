use bevy::prelude::*;
use common::protocol::{Position, ProjectileMarker};

use crate::characters::PreviousTickPosition;

// Update projectile `Transform` from `Position` for rendering. Physics ticks
// at a fixed 30 Hz while rendering runs at the display rate; interpolate
// between the last-tick and current-tick positions using the fixed-step
// overstep fraction so motion stays smooth.
pub fn projectiles_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    mut projectile_query: Query<(&Position, &PreviousTickPosition, &mut Transform), With<ProjectileMarker>>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (pos, prev, mut transform) in &mut projectile_query {
        transform.translation = prev.lerp_to(*pos, alpha);
    }
}
