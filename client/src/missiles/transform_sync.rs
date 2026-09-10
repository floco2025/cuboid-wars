use crate::{
    characters::PreviousTickPosition,
    missiles::{MissileVelocity, OwnedMissile, missile_rotation},
};
use bevy::prelude::*;
use common::protocol::{MissileMarker, Position};

pub(crate) fn missiles_transform_sync_system(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<
        (
            &Position,
            &PreviousTickPosition,
            &MissileVelocity,
            &mut Transform,
            Has<OwnedMissile>,
        ),
        With<MissileMarker>,
    >,
) {
    for (pos, previous, velocity, mut transform, owned) in &mut query {
        transform.translation = if owned {
            previous.lerp_to(*pos, fixed_time.overstep_fraction())
        } else {
            Vec3::from(*pos)
        };
        transform.rotation = missile_rotation(velocity.0);
    }
}
