#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;

use super::{
    effects::{CameraShake, CuboidShake},
    movement::LocalPlayer,
};
use crate::resources::CameraViewMode;
#[allow(clippy::wildcard_imports)]
use crate::{constants::*, spawning::PlayerIdTextMesh};
use common::{
    constants::PLAYER_HEIGHT,
    protocol::{FaceDirection, Position},
    systems::Projectile,
};

// ============================================================================
// Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<(&mut Transform, Option<&CameraShake>), With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    for (mut camera_transform, maybe_shake) in camera_query.iter_mut() {
        match *view_mode {
            CameraViewMode::FirstPerson => {
                camera_transform.translation.x = player_pos.x;
                camera_transform.translation.z = player_pos.z;
                camera_transform.translation.y = PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO;

                if let Some(shake) = maybe_shake {
                    camera_transform.translation.x += shake.offset_x;
                    camera_transform.translation.y += shake.offset_y;
                    camera_transform.translation.z += shake.offset_z;
                }
            }
            CameraViewMode::TopDown => {
                if view_mode.is_changed() {
                    camera_transform.translation = Vec3::new(0.0, TOPDOWN_CAMERA_HEIGHT, TOPDOWN_CAMERA_Z_OFFSET);
                }
                camera_transform.look_at(Vec3::new(TOPDOWN_LOOKAT_X, TOPDOWN_LOOKAT_Y, TOPDOWN_LOOKAT_Z), Vec3::Y);
            }
        }
    }
}

// Update Transform from Position component for rendering
pub fn sync_position_to_transform_system(mut query: Query<(&Position, &mut Transform, Option<&CuboidShake>)>) {
    for (pos, mut transform, maybe_shake) in query.iter_mut() {
        // Base position
        transform.translation.x = pos.x;
        transform.translation.y = PLAYER_HEIGHT / 2.0; // Lift so bottom is at ground (y=0)
        transform.translation.z = pos.z;

        // Apply shake offset if active
        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}

// Update player cuboid rotation from stored face direction component
pub fn sync_face_to_transform_system(mut query: Query<(&FaceDirection, &mut Transform), Without<Camera3d>>) {
    for (face_dir, mut transform) in query.iter_mut() {
        transform.rotation = Quat::from_rotation_y(face_dir.0);
    }
}

// Update projectiles - position updates and despawn
pub fn sync_projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Transform, &mut Projectile)>,
) {
    for (entity, mut transform, mut projectile) in projectile_query.iter_mut() {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        } else {
            transform.translation += projectile.velocity * time.delta_secs();
        }
    }
}

// Update local player visibility based on camera view mode
pub fn sync_local_player_visibility_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<(Entity, &mut Visibility, Has<Mesh3d>), With<LocalPlayer>>,
) {
    // Always check and update, not just when changed, to ensure it's correct
    for (entity, mut visibility, has_mesh) in local_player_query.iter_mut() {
        if view_mode.is_changed() {
            debug!(
                "Local player {:?} has_mesh={} current_visibility={:?} view_mode={:?}",
                entity, has_mesh, *visibility, *view_mode
            );
        }

        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };

        if *visibility != desired_visibility {
            debug!(
                "Updating local player {:?} visibility from {:?} to {:?}",
                entity, *visibility, desired_visibility
            );
            *visibility = desired_visibility;
        }
    }
}

// Make player ID text meshes billboard (always face camera)
pub fn billboard_player_id_text_system(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut text_mesh_query: Query<(&GlobalTransform, &mut Transform), With<PlayerIdTextMesh>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let camera_pos = camera_transform.translation();

    for (global_transform, mut transform) in text_mesh_query.iter_mut() {
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
