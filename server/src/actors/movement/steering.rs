use std::f32::consts::TAU;

use common::{math::angle_delta_radians, protocol::ActorMoveIntent};

const TURN_RATE: f32 = TAU;

pub(super) fn steer(intent: ActorMoveIntent, heading: f32, distance: f32, delta: f32) -> ActorMoveIntent {
    let ActorMoveIntent::Moving { direction, speed } = intent else {
        return intent;
    };
    let error = angle_delta_radians(direction, heading);
    let turn = error.clamp(-TURN_RATE * delta, TURN_RATE * delta);
    // Match travel to facing, pivot for a target behind us, and shrink the
    // turning radius near a corner so we cannot endlessly orbit it.
    ActorMoveIntent::Moving {
        direction: angle_delta_radians(heading + turn, 0.0),
        speed: speed.min(distance * TURN_RATE) * (error - turn).cos().max(0.0),
    }
}
