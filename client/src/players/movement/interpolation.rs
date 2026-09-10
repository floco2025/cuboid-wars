use bevy::prelude::*;
use common::{
    map::Carriers,
    math::angle_delta_radians,
    physics::{
        AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity, player_control_velocity,
    },
    protocol::{
        CarrierId, FaceYaw, MapSettings, PlayerId, PlayerMoveIntent, PlayerMovementState, Position, PowerUpKind,
    },
};

use crate::{
    characters::rendered_carrier_position,
    players::{LocalPlayerMarker, PlayerAnimationMotion, PlayerMap, PlayerSample, RemotePlayerMotion},
};

impl RemotePlayerMotion {
    // The rendered state this frame and the world velocity playback moves at.
    pub(crate) fn advance(
        &mut self,
        delta_ticks: f64,
        carriers: &Carriers,
        carrier_alpha: f32,
        tick_secs: f64,
    ) -> (PlayerMovementState, Vec3) {
        let playback = self.0.advance(delta_ticks);
        let mut movement = rendered(playback.left, carriers, carrier_alpha);
        let Some(right) = playback
            .right
            .filter(|right| right.portal_crossing == playback.left.portal_crossing)
        else {
            return (movement, Vec3::ZERO);
        };
        let start = Vec3::from(movement.pos);
        let end = Vec3::from(rendered(right, carriers, carrier_alpha).pos);
        movement.pos = start.lerp(end, playback.alpha).into();
        movement.face_yaw += angle_delta_radians(right.movement.face_yaw, movement.face_yaw) * playback.alpha;
        movement.vertical_velocity += (right.movement.vertical_velocity - movement.vertical_velocity) * playback.alpha;
        (
            movement,
            (end - start) * (1.0 / (playback.span_ticks * tick_secs)) as f32,
        )
    }
}

fn rendered(sample: &PlayerSample, carriers: &Carriers, carrier_alpha: f32) -> PlayerMovementState {
    let mut movement = sample.movement;
    movement.pos = rendered_carrier_position(movement.carrier, &movement.pos, carriers, carrier_alpha);
    movement.carrier = CarrierId::WORLD;
    movement
}

pub(crate) fn interpolate_remote_players_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    carriers: Res<Carriers>,
    settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    mut query: Query<
        (
            &PlayerId,
            &mut RemotePlayerMotion,
            &mut Position,
            &mut FaceYaw,
            &mut PlayerMoveIntent,
            &mut CharacterVerticalVelocity,
            &mut AirborneMomentum,
            &mut KnockbackVelocity,
            &mut CharacterSupport,
            &mut PlayerAnimationMotion,
        ),
        Without<LocalPlayerMarker>,
    >,
) {
    for (
        id,
        mut buffer,
        mut pos,
        mut yaw,
        mut intent,
        mut vertical,
        mut momentum,
        mut knockback,
        mut support,
        mut animation,
    ) in &mut query
    {
        let (movement, velocity) = buffer.advance(
            time.delta_secs_f64() / fixed_time.timestep().as_secs_f64(),
            &carriers,
            fixed_time.overstep_fraction(),
            fixed_time.timestep().as_secs_f64(),
        );
        *pos = movement.pos;
        yaw.0 = movement.face_yaw;
        *intent = movement.move_intent;
        vertical.0 = movement.vertical_velocity;
        momentum.0 = Vec3::from_array(movement.airborne_momentum);
        knockback.0 = Vec3::from_array(movement.knockback);
        *support = movement.support;
        let info = players.get(id);
        let control = player_control_velocity(
            *intent,
            &settings.movement,
            info.is_some_and(|info| info.power_up(PowerUpKind::Speed)),
            info.is_some_and(|info| info.stunned),
        );
        let direction = control.normalize_or_zero();
        let travelled = velocity - momentum.0 - knockback.0;
        let speed = travelled.dot(direction).clamp(0.0, control.length());
        *animation = PlayerAnimationMotion {
            support: *support,
            velocity: (direction * speed).with_y(vertical.0),
        };
    }
}

#[cfg(test)]
#[path = "tests/interpolation.rs"]
mod tests;
