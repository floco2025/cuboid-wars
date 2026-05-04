use bevy::prelude::*;

use super::components::CuboidShake;
use common::{
    config::GameplayConfig,
    protocol::{PlayerMarker, Position},
};

// ============================================================================
// Transform Sync Systems
// ============================================================================

// Update player Transform from Position component for rendering
pub fn players_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<(&Position, &mut Transform, Option<&CuboidShake>), With<PlayerMarker>>,
) {
    let player_physics = gameplay_config.player.physics();
    for (pos, mut transform, maybe_shake) in &mut player_query {
        // Base position
        transform.translation.x = pos.x;
        transform.translation.y = player_physics.collider_center_y(pos.y);
        transform.translation.z = pos.z;

        // Apply shake offset if active
        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}
