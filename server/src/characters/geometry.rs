use common::{config::CharacterPhysicsConfig, protocol::Position};

// Gap between two capsule surfaces; negative while they overlap.
#[must_use]
pub(crate) fn character_surface_distance(
    a: Position,
    a_physics: CharacterPhysicsConfig,
    b: Position,
    b_physics: CharacterPhysicsConfig,
) -> f32 {
    let ac = a_physics.movement_collider;
    let bc = b_physics.movement_collider;
    let vertical_gap = ((a.y + ac.radius()) - (b.y + bc.height - bc.radius()))
        .max((b.y + bc.radius()) - (a.y + ac.height - ac.radius()))
        .max(0.0);
    (a.horizontal_distance_sq(&b) + vertical_gap * vertical_gap).sqrt() - ac.radius() - bc.radius()
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
