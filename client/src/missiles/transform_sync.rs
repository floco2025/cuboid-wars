use crate::{
    characters::PreviousTickPosition,
    missiles::{MissileMap, MissileVelocity, missile_rotation, resources::OwnedMissile},
};
use bevy::prelude::*;
use common::protocol::{MissileId, MissileMarker, Position};

pub(crate) fn missiles_transform_sync_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    mut missiles: ResMut<MissileMap>,
    mut query: Query<
        (
            &MissileId,
            &mut Position,
            &PreviousTickPosition,
            &mut MissileVelocity,
            &mut Transform,
            Option<&OwnedMissile>,
        ),
        With<MissileMarker>,
    >,
) {
    for (id, mut pos, previous, mut velocity, mut transform, owned) in &mut query {
        if owned.is_some() {
            transform.translation = previous.lerp_to(*pos, fixed_time.overstep_fraction());
        } else if let Some(motion) = missiles.get_mut(id).and_then(|info| info.remote.as_mut()) {
            let movement = motion.advance(time.delta_secs_f64() / fixed_time.timestep().as_secs_f64());
            *pos = movement.pos;
            velocity.0 = movement.velocity();
            transform.translation = Vec3::from(*pos);
        }
        transform.rotation = missile_rotation(velocity.0);
    }
}
