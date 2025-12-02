use bevy::prelude::*;
use std::time::Duration;

use super::network::ServerReconciliation;
use crate::{
    constants::*,
    resources::{CameraViewMode, WallConfig},
    spawning::PlayerIdTextMesh,
};
use common::{
    collision::{calculate_wall_slide, check_player_player_collision, check_player_wall_collision},
    constants::{PLAYER_HEIGHT, RUN_SPEED, UPDATE_BROADCAST_INTERVAL},
    protocol::{FaceDirection, PlayerId, Position, Velocity},
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

// Camera shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32, // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_y: f32,
    pub offset_z: f32,
}

// Cuboid shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CuboidShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32, // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_z: f32,
}

// ============================================================================
// Player Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static Velocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayer>,
    ),
    With<PlayerId>,
>;

#[allow(clippy::similar_names)]
pub fn player_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUIMarker>>,
) {
    let delta = time.delta_secs();
    let entity_positions: Vec<(Entity, Position)> =
        query.iter().map(|(entity, pos, _, _, _, _)| (entity, *pos)).collect();

    for (entity, mut client_pos, client_vel, mut flash_state, mut recon_option, is_local) in &mut query {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let abs_velocity = calculate_absolute_velocity(client_vel);
        let is_standing_still = abs_velocity < f32::EPSILON;

        let target_pos = if let Some(recon) = recon_option.as_mut() {
            const IDLE_CORRECTION_TIME: f32 = 10.0; // Standing still: slow, smooth correction
            const RUN_CORRECTION_TIME: f32 = 0.5; // Running: fast, responsive correction

            let speed_ratio = (abs_velocity / RUN_SPEED).clamp(0.0, 1.0); // Ignore speed power-ups
            let correction_time_interval = IDLE_CORRECTION_TIME.lerp(RUN_CORRECTION_TIME, speed_ratio);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos_x = recon.server_pos.x + recon.server_vel.x * recon.rtt / 2.0;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * recon.rtt / 2.0;

            let total_dx = server_pos_x - recon.client_pos.x;
            let total_dz = server_pos_z - recon.client_pos.z;

            // If the client got totally out of sync, we jump to the server position
            if total_dx >= 5.0 || total_dz >= 5.0 {
                warn!("client out of sync, jumping to server position");
                *client_pos = recon.server_pos;
                continue;
            }

            let dx = total_dx * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
            let dz = total_dz * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

            Position {
                x: client_vel.x.mul_add(delta, client_pos.x) + dx,
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z) + dz,
            }
        } else {
            Position {
                x: client_vel.x.mul_add(delta, client_pos.x),
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z),
            }
        };

        // Skip collision checks if player is standing still
        if is_standing_still {
            *client_pos = target_pos;
            continue;
        }

        let walls = wall_config.as_deref();
        let hits_wall = player_hits_wall(walls, &target_pos);
        let hits_player = player_hits_other_player(entity, &target_pos, &entity_positions);
        if !hits_wall && !hits_player {
            *client_pos = target_pos;
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
        } else if hits_wall {
            // Slide along the wall instead of stopping
            let slide_pos = calculate_wall_slide(
                &walls.expect("walls should exist if hit_wall is true").walls,
                &client_pos,
                &target_pos,
                client_vel.x,
                client_vel.z,
                delta,
            );
            *client_pos = slide_pos;

            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, true);
            }
        } else if hits_player {
            // Stop for player collisions
            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, false);
            }
        }
    }
}

const BUMP_FLASH_DURATION: f32 = 0.08;

fn calculate_absolute_velocity(velocity: &Velocity) -> f32 {
    (velocity.x * velocity.x + velocity.z * velocity.z).sqrt()
}

fn player_hits_wall(walls: Option<&WallConfig>, new_pos: &Position) -> bool {
    let Some(config) = walls else { return false };
    config
        .walls
        .iter()
        .any(|wall| check_player_wall_collision(new_pos, wall))
}

fn player_hits_other_player(entity: Entity, new_pos: &Position, positions: &[(Entity, Position)]) -> bool {
    positions
        .iter()
        .any(|(other_entity, other_pos)| *other_entity != entity && check_player_player_collision(new_pos, other_pos))
}

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUIMarker>>,
) {
    if state.flash_timer <= 0.0 {
        return;
    }

    state.flash_timer -= delta;
    if state.flash_timer <= 0.0
        && is_local
        && let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next()
    {
        *visibility = Visibility::Hidden;
        bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
    }
}

fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUIMarker>>,
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

// ============================================================================
// Visual Effects Systems
// ============================================================================

// Apply camera shake effect - updates shake offset
pub fn apply_camera_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(Entity, &mut CameraShake), With<Camera3d>>,
) {
    for (entity, mut shake) in &mut camera_query {
        update_camera_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

// Apply cuboid shake effect - updates shake offset
pub fn apply_cuboid_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cuboid_query: Query<(Entity, &mut CuboidShake)>,
) {
    for (entity, mut shake) in &mut cuboid_query {
        update_cuboid_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

fn update_camera_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CameraShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CameraShake>();
        return;
    }

    let progress = shake.timer.fraction();
    let amplitude = shake.intensity * (1.0 - progress);
    let oscillation = (progress * 30.0).sin();

    shake.offset_x = shake.dir_x * amplitude * oscillation;
    shake.offset_z = shake.dir_z * amplitude * oscillation;
    shake.offset_y = amplitude * oscillation * 0.2;
}

fn update_cuboid_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CuboidShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CuboidShake>();
        return;
    }

    let progress = shake.timer.fraction();
    let amplitude = shake.intensity * (1.0 - progress);
    let bounce = (progress * 20.0).sin();

    shake.offset_x = shake.dir_x * amplitude * bounce;
    shake.offset_z = shake.dir_z * amplitude * bounce;
}

// ============================================================================
// Player Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<(&mut Transform, &mut Projection, Option<&CameraShake>), With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    for (mut camera_transform, mut projection, maybe_shake) in &mut camera_query {
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

                // Set FPV FOV
                if let Projection::Perspective(persp) = projection.as_mut() {
                    persp.fov = FPV_CAMERA_FOV_DEGREES.to_radians();
                }
            }
            CameraViewMode::TopDown => {
                if view_mode.is_changed() {
                    camera_transform.translation = Vec3::new(0.0, TOPDOWN_CAMERA_HEIGHT, TOPDOWN_CAMERA_Z_OFFSET);
                }
                camera_transform.look_at(Vec3::new(TOPDOWN_LOOKAT_X, TOPDOWN_LOOKAT_Y, TOPDOWN_LOOKAT_Z), Vec3::Y);

                // Set top-down FOV
                if let Projection::Perspective(persp) = projection.as_mut() {
                    persp.fov = TOPDOWN_CAMERA_FOV_DEGREES.to_radians();
                }
            }
        }
    }
}

// Update player Transform from Position component for rendering
pub fn sync_position_to_transform_system(
    mut player_query: Query<(&Position, &mut Transform, Option<&CuboidShake>), With<PlayerId>>,
) {
    for (pos, mut transform, maybe_shake) in &mut player_query {
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
    for (face_dir, mut transform) in &mut query {
        transform.rotation = Quat::from_rotation_y(face_dir.0);
    }
}

// Update local player visibility based on camera view mode
pub fn sync_local_player_visibility_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<(Entity, &mut Visibility, Has<Mesh3d>), With<LocalPlayer>>,
) {
    // Always check and update, not just when changed, to ensure it's correct
    for (entity, mut visibility, has_mesh) in &mut local_player_query {
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
// Billboard System
// ============================================================================

// Make player ID text meshes billboard (always face camera)
pub fn player_billboard_system(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut text_mesh_query: Query<(&GlobalTransform, &mut Transform), With<PlayerIdTextMesh>>,
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
