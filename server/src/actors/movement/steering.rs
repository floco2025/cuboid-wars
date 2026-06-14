use common::{
    math::angle_delta_radians,
    protocol::{ActorMoveIntent, Position},
};

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

fn direction_toward(pos: &Position, target: &Position) -> f32 {
    let dx = target.x - pos.x;
    let dz = target.z - pos.z;
    dx.atan2(dz)
}

// Smallest absolute angle between two headings, in `[0, PI]`.
pub fn angular_distance(a: f32, b: f32) -> f32 {
    angle_delta_radians(a, b).abs()
}
