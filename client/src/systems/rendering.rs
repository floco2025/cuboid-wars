use bevy::prelude::*;
use common::protocol::{Movement, Position};

use crate::components::LocalPlayer;

// ============================================================================
// Rendering Systems
// ============================================================================

// Player dimensions - used for rendering and camera positioning
pub const PLAYER_HEIGHT: f32 = 80.0; // up/down

// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    if let Some(pos) = local_player_query.iter().next() {
        for mut camera_transform in camera_query.iter_mut() {
            camera_transform.translation.x = pos.x as f32 / 1000.0;
            camera_transform.translation.z = pos.y as f32 / 1000.0;
            camera_transform.translation.y = 72.0; // 90% of 80 unit height (units are mm, but rendering in weird scale)
        }
    }
}

// Update Transform from Position component for rendering
// Position is in millimeters, Transform is in meters
pub fn sync_position_to_transform_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x as f32 / 1000.0; // mm to meters
        transform.translation.z = pos.y as f32 / 1000.0; // mm to meters
    }
}

// Update player cuboid rotation from stored movement component
pub fn sync_rotation_to_transform_system(mut query: Query<(&Movement, &mut Transform), Without<Camera3d>>) {
    for (mov, mut transform) in query.iter_mut() {
        // Face direction: 0 = facing -Y direction
        // Add π to flip the model 180° so nose points in the right direction
        transform.rotation = Quat::from_rotation_y(mov.face_dir + std::f32::consts::PI);
    }
}
