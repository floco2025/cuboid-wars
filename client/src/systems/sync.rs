#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::time::Duration;

use super::effects::{CameraShake, CuboidShake};
#[allow(clippy::wildcard_imports)]
use crate::constants::*;
use crate::resources::{CameraViewMode, WallConfig};
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

// Server's authoritative snapshot for this entity
#[derive(Component)]
pub struct ServerSnapshot {
    pub pos: Position,
    pub speed: common::protocol::Speed,
    pub received_at: Duration, // Time::elapsed() when snapshot was received
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

// Client-side movement system with wall collision detection for smooth prediction
pub fn client_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    mut query: Query<(
        Entity,
        &mut Position,
        &Velocity,
        Option<&mut BumpFlashState>,
        Option<&ServerSnapshot>,
        Has<LocalPlayer>,
    )>,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    let delta = time.delta_secs();

    let walls = wall_config.as_deref();
    let entity_positions: Vec<(Entity, Position)> =
        query.iter().map(|(entity, pos, _, _, _, _)| (entity, *pos)).collect();

    for (entity, mut pos, velocity, mut flash_state, server_snapshot, is_local) in query.iter_mut() {
        // Calculate how old the server snapshot is
        let _server_age = if let Some(snapshot) = server_snapshot {
            time.elapsed() - snapshot.received_at
        } else {
            Duration::ZERO
        };

        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        if !has_horizontal_velocity(velocity) {
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
            continue;
        }

        let target_pos = Position {
            x: pos.x + velocity.x * delta,
            y: pos.y,
            z: pos.z + velocity.z * delta,
        };

        let hit_wall = hits_wall(walls, &target_pos);
        let hit_player = hits_other_player(entity, &target_pos, &entity_positions);
        let blocked = hit_wall || hit_player;

        if !blocked {
            *pos = target_pos;
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
        } else if is_local {
            if let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, hit_wall);
            }
        }
    }
}

const BUMP_FLASH_DURATION: f32 = 0.08;

fn has_horizontal_velocity(velocity: &Velocity) -> bool {
    velocity.x.abs() > f32::EPSILON || velocity.z.abs() > f32::EPSILON
}

fn hits_wall(walls: Option<&WallConfig>, new_pos: &Position) -> bool {
    let Some(config) = walls else { return false };
    config
        .walls
        .iter()
        .any(|wall| common::collision::check_player_wall_collision(new_pos, wall))
}

fn hits_other_player(entity: Entity, new_pos: &Position, positions: &[(Entity, Position)]) -> bool {
    positions.iter().any(|(other_entity, other_pos)| {
        *other_entity != entity && common::collision::check_player_player_collision(new_pos, other_pos)
    })
}

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    if state.flash_timer <= 0.0 {
        return;
    }

    state.flash_timer -= delta;
    if state.flash_timer <= 0.0 && is_local {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Hidden;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
        }
    }
}

fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
    state: &mut Mut<BumpFlashState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Visible;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
        }

        let sound_path = if collided_with_wall {
            "sounds/player_bumps_wall.ogg"
        } else {
            "sounds/player_bumps_player.ogg"
        };

        commands.spawn((
            AudioPlayer::new(asset_server.load(sound_path)),
            PlaybackSettings::DESPAWN,
        ));

        state.flash_timer = BUMP_FLASH_DURATION;
    }

    state.was_colliding = true;
}
