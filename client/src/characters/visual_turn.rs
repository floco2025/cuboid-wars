use bevy::prelude::*;
use common::{
    math::angle_delta_radians,
    protocol::{ActorMarker, FaceYaw, PlayerMarker},
};

use crate::{actors::FixedFacingMarker, constants::CHARACTER_VISUAL_TURN_MAX_SPEED};

// Smoothly rotate the rendered character yaw toward the gameplay `FaceYaw`
// at a capped angular speed. `FaceYaw` itself stays immediate (shooting
// and networking read it); only the visual lags behind it. The speed cap is the
// whole point: however fast the server flips an actor's facing (e.g. an AI
// thrashing while it can't reach a target), the model can only turn this much
// per second, so it never spins — facing smoothness is decoupled from the AI.
pub fn characters_visual_turn_system(
    time: Res<Time>,
    mut query: Query<
        (&FaceYaw, &mut Transform),
        (
            Or<(With<PlayerMarker>, With<ActorMarker>)>,
            Without<Camera3d>,
            Without<FixedFacingMarker>,
        ),
    >,
) {
    let max_step = CHARACTER_VISUAL_TURN_MAX_SPEED * time.delta_secs();
    for (face_yaw, mut transform) in &mut query {
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        transform.rotation = Quat::from_rotation_y(step_yaw_toward(current_yaw, face_yaw.0, max_step));
    }
}

// Move `current` toward `target` (radians) by at most `max_step`, taking the
// shortest way around the circle; snaps to `target` once within range.
fn step_yaw_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = angle_delta_radians(target, current);
    if delta.abs() <= max_step {
        target
    } else {
        current + delta.signum() * max_step
    }
}

#[cfg(test)]
#[path = "tests/visual_turn.rs"]
mod tests;
