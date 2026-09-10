use bevy::prelude::*;
use common::{
    map::Carriers,
    math::angle_delta_radians,
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{ActorMoveIntent, ActorMovementState, CarrierId, FaceYaw, Position},
};

use super::{ActorAnimationVelocity, RemoteActorMotion};
use crate::characters::rendered_carrier_position;

impl RemoteActorMotion {
    // The rendered state this frame and the world velocity playback moves at.
    pub(crate) fn advance(
        &mut self,
        delta_ticks: f64,
        carriers: &Carriers,
        carrier_alpha: f32,
        tick_secs: f64,
    ) -> (ActorMovementState, Vec3) {
        let playback = self.0.advance(delta_ticks);
        let mut movement = rendered(playback.left, carriers, carrier_alpha);
        let Some(right) = playback.right else {
            return (movement, Vec3::ZERO);
        };
        let start = Vec3::from(movement.pos);
        let end = Vec3::from(rendered(right, carriers, carrier_alpha).pos);
        movement.pos = start.lerp(end, playback.alpha).into();
        movement.face_yaw += angle_delta_radians(right.face_yaw, movement.face_yaw) * playback.alpha;
        movement.vertical_velocity += (right.vertical_velocity - movement.vertical_velocity) * playback.alpha;
        (
            movement,
            (end - start) * (1.0 / (playback.span_ticks * tick_secs)) as f32,
        )
    }
}

fn rendered(sample: &ActorMovementState, carriers: &Carriers, carrier_alpha: f32) -> ActorMovementState {
    let mut movement = *sample;
    movement.pos = rendered_carrier_position(movement.carrier, &movement.pos, carriers, carrier_alpha);
    movement.carrier = CarrierId::WORLD;
    movement
}

pub(crate) fn interpolate_remote_actors_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    carriers: Res<Carriers>,
    mut query: Query<(
        &mut RemoteActorMotion,
        &mut Position,
        &mut FaceYaw,
        &mut ActorMoveIntent,
        &mut CharacterVerticalVelocity,
        &mut CharacterSupport,
        &mut ActorAnimationVelocity,
    )>,
) {
    for (mut buffer, mut pos, mut yaw, mut intent, mut vertical, mut support, mut velocity) in &mut query {
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
    }
}

#[cfg(test)]
#[path = "tests/interpolation.rs"]
mod tests;
