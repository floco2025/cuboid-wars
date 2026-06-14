use bevy::prelude::*;
use common::{
    math::angle_delta_radians,
    protocol::{ActorMarker, FaceDirection, PlayerMarker},
};

use crate::constants::CHARACTER_VISUAL_TURN_MAX_SPEED;

// Smoothly rotate the rendered character yaw toward the gameplay `FaceDirection`
// at a capped angular speed. `FaceDirection` itself stays immediate (shooting
// and networking read it); only the visual lags behind it. The speed cap is the
// whole point: however fast the server flips an actor's facing (e.g. an AI
// thrashing while it can't reach a target), the model can only turn this much
// per second, so it never spins — facing smoothness is decoupled from the AI.
pub fn characters_visual_turn_system(
    time: Res<Time>,
    mut query: Query<
        (&FaceDirection, &mut Transform),
        (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<Camera3d>),
    >,
) {
    let max_step = CHARACTER_VISUAL_TURN_MAX_SPEED * time.delta_secs();
    for (face_dir, mut transform) in &mut query {
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        transform.rotation = Quat::from_rotation_y(step_yaw_toward(current_yaw, face_dir.0, max_step));
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
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn aligned(a: f32, b: f32) -> bool {
        angle_delta_radians(a, b).abs() < 1e-4
    }

    #[test]
    fn snaps_when_within_one_step() {
        assert!(aligned(step_yaw_toward(0.0, 0.1, 0.5), 0.1));
    }

    #[test]
    fn caps_a_large_flip_to_max_step() {
        // Target nearly opposite: a single step advances exactly `max_step`.
        assert!(aligned(step_yaw_toward(0.0, PI - 0.01, 0.2), 0.2));
    }

    #[test]
    fn takes_the_shortest_way_around_the_wrap() {
        // current +3.0, target -3.0: shortest path is ~+0.28 across ±PI, not -6.
        let stepped = step_yaw_toward(3.0, -3.0, 0.1);
        assert!(aligned(stepped, 3.1), "advances 0.1 the short way (wrapping)");
        assert!(
            angle_delta_radians(-3.0, stepped).abs() < angle_delta_radians(-3.0, 3.0).abs(),
            "ended up closer to the target"
        );
    }
}
