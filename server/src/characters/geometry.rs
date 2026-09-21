use common::{config::CharacterPhysicsConfig, physics::character_axis_separation, protocol::Position};

// Gap between two capsule surfaces; negative while they overlap.
#[must_use]
pub(crate) fn character_surface_distance(
    a: Position,
    a_physics: CharacterPhysicsConfig,
    b: Position,
    b_physics: CharacterPhysicsConfig,
) -> f32 {
    character_axis_separation(&a, a_physics, &b, b_physics).length()
        - a_physics.movement_collider.radius()
        - b_physics.movement_collider.radius()
}

#[must_use]
pub(crate) fn character_overlaps_item(character_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = character_pos.x - item_pos.x;
    let dy = character_pos.y - item_pos.y;
    let dz = character_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
