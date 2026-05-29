use rand::{RngExt, rngs::ThreadRng};

use common::protocol::{ActorMoveIntent, Position};

pub fn actor_desired_intent(
    go_to_position: &mut Option<Position>,
    pos: &Position,
    reached_distance: f32,
    speed: f32,
) -> Option<ActorMoveIntent> {
    let target_pos = *go_to_position.as_ref()?;
    if pos.horizontal_distance_sq(&target_pos) <= reached_distance * reached_distance {
        *go_to_position = None;
        return None;
    }

    Some(ActorMoveIntent::Moving {
        direction: direction_toward(pos, &target_pos),
        speed,
    })
}

pub fn steering_directions(direction: f32, side: f32) -> [f32; 7] {
    [
        direction,
        direction + side * 20.0_f32.to_radians(),
        direction + side * 45.0_f32.to_radians(),
        direction + side * 90.0_f32.to_radians(),
        direction - side * 20.0_f32.to_radians(),
        direction - side * 45.0_f32.to_radians(),
        direction - side * 90.0_f32.to_radians(),
    ]
}

pub fn random_avoidance_side(rng: &mut ThreadRng) -> f32 {
    if rng.random_bool(0.5) { 1.0 } else { -1.0 }
}

fn direction_toward(pos: &Position, target: &Position) -> f32 {
    let dx = target.x - pos.x;
    let dz = target.z - pos.z;
    dx.atan2(dz)
}
