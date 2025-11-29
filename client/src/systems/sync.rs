#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;

use super::{
    effects::{CameraShake, CuboidShake},
    movement::LocalPlayer,
};
#[allow(clippy::wildcard_imports)]
use crate::constants::*;
use crate::resources::CameraViewMode;
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
