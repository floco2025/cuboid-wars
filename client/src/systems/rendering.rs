use bevy::prelude::*;
use common::constants::PLAYER_HEIGHT;
use common::protocol::{Movement, Position};

use crate::components::LocalPlayer;

// ============================================================================
// Rendering Systems
// ============================================================================

// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    if let Some(pos) = local_player_query.iter().next() {
        for mut camera_transform in camera_query.iter_mut() {
            camera_transform.translation.x = pos.x;
            camera_transform.translation.z = pos.z;
            camera_transform.translation.y = PLAYER_HEIGHT * 0.9; // 90% of player height
        }
    }
}

// Update Transform from Position component for rendering
// Both Position and Transform use meters now
pub fn sync_position_to_transform_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x;
        transform.translation.y = PLAYER_HEIGHT / 2.0; // Lift so bottom is at ground (y=0)
        transform.translation.z = pos.z;
    }
}

// Update player cuboid rotation from stored movement component
pub fn sync_rotation_to_transform_system(mut query: Query<(&Movement, &mut Transform), Without<Camera3d>>) {
    for (mov, mut transform) in query.iter_mut() {
        transform.rotation = Quat::from_rotation_y(mov.face_dir);
    }
}
