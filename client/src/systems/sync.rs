#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use super::effects::{CameraShake, CuboidShake};
#[allow(clippy::wildcard_imports)]
use crate::constants::*;
use crate::resources::{CameraViewMode, PastPosVel};
use common::{
    constants::PLAYER_HEIGHT,
    protocol::{FaceDirection, Position, Velocity},
    systems::Projectile,
};

// ============================================================================
// Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
}

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
                }
                CameraViewMode::TopDown => {
                    // When view mode just changed, position camera to the side of the field
                    if view_mode.is_changed() {
                        camera_transform.translation = Vec3::new(
                            0.0, // Center on X axis (side view)
                            TOPDOWN_CAMERA_HEIGHT,
                            TOPDOWN_CAMERA_Z_OFFSET, // Distance from center along Z
                        );
                    }
                    // Always look at the center of the field (0, 0)
                    camera_transform.look_at(Vec3::new(TOPDOWN_LOOKAT_X, TOPDOWN_LOOKAT_Y, TOPDOWN_LOOKAT_Z), Vec3::Y);
                }
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

// ============================================================================
// Client Movement System (with Wall Collision)
// ============================================================================

// Client-side movement system with wall collision detection for smooth prediction
pub fn client_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<crate::resources::WallConfig>>,
    mut past_pos_vel: ResMut<PastPosVel>,
    rtt: Res<crate::resources::RoundTripTime>,
    mut timer: Local<f32>,
    mut query: Query<(
        Entity,
        &mut Position,
        &Velocity,
        Option<&mut BumpFlashState>,
        Has<LocalPlayer>,
    )>,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    let delta = time.delta_secs();

    // Update timer for snapshot (in seconds)
    *timer += delta;

    // Collect all current positions for player-player collision checks
    let positions: Vec<(Entity, Position)> = query.iter().map(|(entity, pos, _, _, _)| (entity, *pos)).collect();

    for (entity, mut pos, velocity, mut flash_state, is_local) in query.iter_mut() {
        // Tick down flash timer (only for local player)
        if let Some(ref mut state) = flash_state {
            if state.flash_timer > 0.0 {
                state.flash_timer -= delta;
                if state.flash_timer <= 0.0 && is_local {
                    // Flash finished, hide it
                    if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
                        *visibility = Visibility::Hidden;
                        bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
                    }
                }
            }
        }

        // Check if moving (velocity magnitude > 0)
        let speed_magnitude = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();

        if speed_magnitude > 0.0 {
            // Calculate new position using velocity components
            let new_pos = Position {
                x: pos.x + velocity.x * delta,
                y: pos.y,
                z: pos.z + velocity.z * delta,
            };

            // Check if new position collides with any wall (if walls are loaded)
            let collides_with_wall = if let Some(wall_config) = wall_config.as_ref() {
                wall_config
                    .walls
                    .iter()
                    .any(|wall| common::collision::check_player_wall_collision(&new_pos, wall))
            } else {
                false // No walls loaded yet, allow movement
            };

            // Check if new position collides with any other player
            let collides_with_player = positions.iter().any(|(other_entity, other_pos)| {
                *other_entity != entity && common::collision::check_player_player_collision(&new_pos, other_pos)
            });

            // Only update position if no collision
            if !collides_with_wall && !collides_with_player {
                *pos = new_pos;

                if let Some(ref mut state) = flash_state {
                    state.was_colliding = false;
                }
            } else if is_local {
                // Collision detected for local player - trigger flash and sound on NEW collision
                if let Some(ref mut state) = flash_state {
                    if !state.was_colliding {
                        // Trigger flash
                        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
                            *visibility = Visibility::Visible;
                            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
                            state.flash_timer = 0.08; // Flash duration
                        }

                        // Play appropriate collision sound
                        let sound_path = if collides_with_wall {
                            "sounds/player_bumps_wall.ogg"
                        } else {
                            "sounds/player_bumps_player.ogg"
                        };

                        commands.spawn((
                            AudioPlayer::new(asset_server.load(sound_path)),
                            PlaybackSettings::DESPAWN,
                        ));
                    }
                    state.was_colliding = true;
                }
            }
        } else if let Some(ref mut state) = flash_state {
            // Not moving, reset flash state
            state.was_colliding = false;
        }

        // Save snapshot of local player position and velocity every RTT seconds
        if is_local && *timer >= rtt.rtt as f32 {
            past_pos_vel.pos = *pos;
            past_pos_vel.vel = *velocity;
            past_pos_vel.timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
            *timer = 0.0;
        }
    }
}
