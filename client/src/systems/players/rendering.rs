use bevy::prelude::*;

use super::components::CuboidShake;
use crate::markers::*;
use common::{
    constants::PLAYER_HEIGHT,
    markers::PlayerMarker,
    protocol::{FaceDirection, Position},
};

// ============================================================================
// Transform Sync Systems
// ============================================================================

// Update player Transform from Position component for rendering
pub fn players_transform_sync_system(
    mut player_query: Query<(&Position, &mut Transform, Option<&CuboidShake>), With<PlayerMarker>>,
) {
    for (pos, mut transform, maybe_shake) in &mut player_query {
        // Base position
        transform.translation.x = pos.x;
        transform.translation.y = pos.y + PLAYER_HEIGHT / 2.0; // Center the cuboid
        transform.translation.z = pos.z;

        // Apply shake offset if active
        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}

// Update player cuboid rotation from stored face direction component
// This query matches both players and sentries (both have FaceDirection)
// but excludes cameras to avoid conflicts
pub fn players_face_to_transform_system(mut query: Query<(&FaceDirection, &mut Transform), Without<Camera3d>>) {
    for (face_dir, mut transform) in &mut query {
        transform.rotation = Quat::from_rotation_y(face_dir.0);
    }
}

// ============================================================================
// Billboard System
// ============================================================================

// Make player ID text meshes billboard (always face camera)
pub fn players_billboard_system(
    camera_query: Query<&GlobalTransform, (With<Camera3d>, Without<RearviewCameraMarker>)>,
    mut text_mesh_query: Query<(&GlobalTransform, &mut Transform), With<PlayerIdTextMeshMarker>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let camera_pos = camera_transform.translation();

    for (global_transform, mut transform) in &mut text_mesh_query {
        let text_pos = global_transform.translation();
        // Calculate direction to camera on XZ plane only (keep Y upright)
        let direction = Vec3::new(camera_pos.x - text_pos.x, 0.0, camera_pos.z - text_pos.z);
        if direction.length_squared() > 0.0001 {
            // Calculate world rotation needed to face camera
            let world_rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));

            // Get the combined parent rotation from global transform
            let global_rotation = global_transform.to_scale_rotation_translation().1;
            // Extract just the Y rotation from global
            let global_y_angle = global_rotation.to_euler(EulerRot::YXZ).0;
            // Calculate what the local Y rotation is currently
            let local_y_angle = transform.rotation.to_euler(EulerRot::YXZ).0;
            // Parent Y rotation is the difference
            let parent_y_angle = global_y_angle - local_y_angle;

            // Calculate new local rotation that compensates for parent
            let world_y_angle = world_rotation.to_euler(EulerRot::YXZ).0;
            let new_local_y_angle = world_y_angle - parent_y_angle;
            transform.rotation = Quat::from_rotation_y(new_local_y_angle);
        }
    }
}
