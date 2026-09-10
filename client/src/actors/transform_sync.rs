use bevy::prelude::*;

use super::{ActorAnimationVelocity, RemoteActorMotion};
use common::{
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{ActorMoveIntent, FaceYaw, Position},
};

pub(crate) fn actors_transform_sync_system(
    time: Res<Time>,
    carriers: Res<Carriers>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(
        &mut RemoteActorMotion,
        &mut Position,
        &mut FaceYaw,
        &mut ActorMoveIntent,
        &mut CharacterVerticalVelocity,
        &mut CharacterSupport,
        &mut ActorAnimationVelocity,
        &mut Transform,
    )>,
) {
    for (mut buffer, mut pos, mut yaw, mut intent, mut vertical, mut support, mut velocity, mut transform) in &mut query
    {
        let (movement, travel) = buffer.advance(
            time.delta_secs_f64() / fixed_time.timestep().as_secs_f64(),
            &carriers,
            fixed_time.overstep_fraction(),
            fixed_time.timestep().as_secs_f64(),
        );
        *pos = movement.pos;
        yaw.0 = movement.face_yaw;
        *intent = movement.move_intent;
        vertical.0 = movement.vertical_velocity;
        *support = movement.support;
        velocity.0 = travel;
        transform.translation = Vec3::from(*pos);
    }
}
