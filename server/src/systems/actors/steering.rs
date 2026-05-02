use rand::{RngExt, rngs::ThreadRng};

use common::protocol::{CharacterMoveIntent, Position};

pub fn actor_desired_intent(
    go_to_position: &mut Option<Position>,
    pos: &Position,
    reached_distance: f32,
) -> Option<CharacterMoveIntent> {
    let target_pos = *go_to_position.as_ref()?;
    if horizontal_distance_sq(pos, &target_pos) <= reached_distance * reached_distance {
        *go_to_position = None;
        return None;
    }

    Some(CharacterMoveIntent::Moving {
        direction: direction_toward(pos, &target_pos),
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

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}
