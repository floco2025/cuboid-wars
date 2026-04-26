use crate::protocol::Position;

#[must_use]
pub fn overlap_player_vs_item(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dz * dz);
    dist_sq <= collection_radius * collection_radius
}
