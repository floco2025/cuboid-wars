use crate::protocol::Position;

#[must_use]
pub fn overlap_player_vs_item(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dy = player_pos.y - item_pos.y;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}
