use crate::protocol::Position;

#[must_use]
pub fn character_overlaps_item(character_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = character_pos.x - item_pos.x;
    let dy = character_pos.y - item_pos.y;
    let dz = character_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}
