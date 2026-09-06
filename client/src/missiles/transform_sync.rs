use bevy::prelude::*;

use crate::{
    characters::PreviousTickPosition,
    missiles::{MissileVelocity, spawn::missile_rotation},
};
use common::protocol::{MissileMarker, Position};

// Render-rate interpolation between the last two fixed-tick positions, plus
// nose-along-velocity orientation (root-only Transform writes; the mesh
// children inherit).
pub fn missiles_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&Position, &PreviousTickPosition, &MissileVelocity, &mut Transform), With<MissileMarker>>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (pos, prev, velocity, mut transform) in &mut query {
        transform.translation = prev.lerp_to(*pos, alpha);
        let rotation = missile_rotation(velocity.0);
        if rotation != Quat::IDENTITY {
            transform.rotation = rotation;
        }
    }
}
