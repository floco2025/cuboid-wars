use common::{
    config::CharacterPhysicsConfig, constants::ITEM_HOVER_HEIGHT, physics::character_axis_separation,
    protocol::Position,
};

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

// Measured from the hovering item to the body's axis, feet to head, so a body
// falling or jumping through an item takes it as one walking into it does.
// The hover height exceeds the collection radius, so a body under the item's
// floor cannot reach it through the slab.
#[must_use]
pub(crate) fn character_overlaps_item(
    character_pos: &Position,
    physics: CharacterPhysicsConfig,
    item_pos: &Position,
    collection_radius: f32,
) -> bool {
    let item_y = item_pos.y + ITEM_HOVER_HEIGHT;
    let dx = character_pos.x - item_pos.x;
    let dy = item_y - item_y.clamp(character_pos.y, character_pos.y + physics.movement_collider.height);
    let dz = character_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
