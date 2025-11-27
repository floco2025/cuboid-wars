use bevy::prelude::*;
use common::systems::Projectile;
use common::constants::PLAYER_HEIGHT;
use common::protocol::{Movement, Position};
use super::effects::{CameraShake, CuboidShake};
use crate::resources::CameraViewMode;

// ============================================================================
// Camera Settings
// ============================================================================

// First-person view camera settings
const FPV_CAMERA_HEIGHT_RATIO: f32 = 0.9; // Camera height as ratio of player height (0.9 = 90% = eye level)

// Top-down view camera settings
const TOPDOWN_CAMERA_HEIGHT: f32 = 30.0;     // Height above ground (meters)
const TOPDOWN_CAMERA_DISTANCE: f32 = 20.0;   // Distance behind player (meters)
const TOPDOWN_LOOKAT_HEIGHT: f32 = 1.0;      // Height to look at (player level)

// ============================================================================
// Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// ============================================================================
// Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<(&mut Transform, Option<&CameraShake>), With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    if let Some(pos) = local_player_query.iter().next() {
        for (mut camera_transform, maybe_shake) in camera_query.iter_mut() {
            match *view_mode {
                CameraViewMode::FirstPerson => {
                    // First person view - camera at eye level, follows player
                    camera_transform.translation.x = pos.x;
                    camera_transform.translation.z = pos.z;
                    camera_transform.translation.y = PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO;
                    
                    // Apply shake offset if active
                    if let Some(shake) = maybe_shake {
                        camera_transform.translation.x += shake.offset_x;
                        camera_transform.translation.y += shake.offset_y;
                        camera_transform.translation.z += shake.offset_z;
                    }
                },
                CameraViewMode::TopDown => {
                    // When view mode just changed, position camera above and behind player
                    if view_mode.is_changed() {
                        camera_transform.translation = Vec3::new(
                            pos.x,
                            TOPDOWN_CAMERA_HEIGHT,
                            pos.z + TOPDOWN_CAMERA_DISTANCE
                        );
                        camera_transform.look_at(
                            Vec3::new(pos.x, TOPDOWN_LOOKAT_HEIGHT, pos.z),
                            Vec3::Y
                        );
                    }
                    // Otherwise don't move the camera - let player control it with input
                },
            }
        }
    }
}

// Update Transform from Position component for rendering
// Both Position and Transform use meters now
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

// Update player cuboid rotation from stored movement component
pub fn sync_rotation_to_transform_system(mut query: Query<(&Movement, &mut Transform), Without<Camera3d>>) {
    for (mov, mut transform) in query.iter_mut() {
        transform.rotation = Quat::from_rotation_y(mov.face_dir);
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
            debug!("Local player {:?} has_mesh={} current_visibility={:?} view_mode={:?}", 
                   entity, has_mesh, *visibility, *view_mode);
        }
        
        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };
        
        if *visibility != desired_visibility {
            debug!("Updating local player {:?} visibility from {:?} to {:?}", entity, *visibility, desired_visibility);
            *visibility = desired_visibility;
        }
    }
}
