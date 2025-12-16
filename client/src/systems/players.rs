use bevy::{camera::Viewport, prelude::*};
use std::time::Duration;

use super::{network::ServerReconciliation, ui::BumpFlashUIMarker};
use crate::{
    constants::*,
    resources::{CameraViewMode, PlayerMap, WallConfig},
    spawning::PlayerIdTextMeshMarker,
};
use common::{
    collision::{calculate_wall_slide, check_player_wall_sweep},
    constants::{PLAYER_HEIGHT, SPEED_RUN, UPDATE_BROADCAST_INTERVAL},
    markers::PlayerMarker,
    players::{PlannedMove, overlaps_other_player},
    protocol::{FaceDirection, PlayerId, Position, Velocity, Wall},
};

// ============================================================================
// Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayerMarker;

// Marker component for the main camera
#[derive(Component)]
pub struct MainCameraMarker;

// Marker component for the rearview mirror camera
#[derive(Component)]
pub struct RearviewCameraMarker;

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
}

// ============================================================================
// Query Bundles
// ============================================================================

// Common query for player movement (read-only)
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub struct PlayerMovement {
    pub position: &'static Position,
    pub face_direction: &'static FaceDirection,
}

// Common query for player movement (mutable)
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub struct PlayerMovementMut {
    pub velocity: &'static mut Velocity,
    pub face_direction: &'static mut FaceDirection,
}

// ============================================================================
// Camera and Visual Effects
// ============================================================================

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
// Helper Functions
// ============================================================================

const BUMP_FLASH_DURATION: f32 = 0.08;

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
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
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
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
// Players Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static Velocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayerMarker>,
    ),
>;

pub fn players_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    players: Res<PlayerMap>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position, then apply wall collision logic
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, player_id, mut client_pos, client_vel, mut flash_state, mut recon_option, is_local) in &mut query
    {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let abs_velocity = client_vel.x.hypot(client_vel.z);
        let is_standing_still = abs_velocity < f32::EPSILON;

        // Calculate intended position from velocity (with server reconciliation if needed)
        let target_pos = if let Some(recon) = recon_option.as_mut() {
            const IDLE_CORRECTION_TIME: f32 = 10.0; // Standing still: slow, smooth correction
            const RUN_CORRECTION_TIME: f32 = 0.5; // Running: fast, responsive correction

            let speed_ratio = (abs_velocity / SPEED_RUN).clamp(0.0, 1.0); // Ignore speed power-ups
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
            let out_of_sync_distance = if is_standing_still { 2.0 } else { 5.0 };
            if total_dx.abs() >= out_of_sync_distance || total_dz.abs() >= out_of_sync_distance {
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
            planned_moves.push(PlannedMove {
                entity,
                target: target_pos,
                hits_wall: false,
            });
            continue;
        }

        // Wall collision - Select walls based on phasing power-up
        let has_phasing = players.0.get(player_id).is_some_and(|info| info.phasing_power_up);

        let (wall_adjusted_target, hits_wall) = if let Some(config) = wall_config.as_ref() {
            let walls_to_check: &[Wall] = if has_phasing {
                &config.boundary_walls
            } else {
                &config.all_walls
            };

            // Check wall collision and calculate target (with sliding if hit)
            if walls_to_check
                .iter()
                .any(|wall| check_player_wall_sweep(&client_pos, &target_pos, wall))
            {
                (
                    calculate_wall_slide(walls_to_check, &client_pos, client_vel.x, client_vel.z, delta),
                    true,
                )
            } else {
                (target_pos, false)
            }
        } else {
            (target_pos, false)
        };

        planned_moves.push(PlannedMove {
            entity,
            target: wall_adjusted_target,
            hits_wall,
        });
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        let Ok((_, _, mut client_pos, _, mut flash_state, _, is_local)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        let hits_player = overlaps_other_player(planned_move, &planned_moves);

        // Apply final position and feedback
        if hits_player {
            // Stop for player collisions
            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, false);
            }
        } else {
            *client_pos = planned_move.target;

            if let Some(state) = flash_state.as_mut() {
                if planned_move.hits_wall {
                    if is_local {
                        trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, true);
                    }
                } else {
                    state.was_colliding = false;
                }
            }
        }
    }
}

// ============================================================================
// Visual Effects Systems
// ============================================================================

// Apply camera shake effect - updates shake offset
pub fn local_player_camera_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(Entity, &mut CameraShake), With<Camera3d>>,
) {
    for (entity, mut shake) in &mut camera_query {
        update_camera_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

// Apply cuboid shake effect - updates shake offset
pub fn local_player_cuboid_shake_system(
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
// Players Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn local_player_camera_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
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

// Update local player visibility based on camera view mode
pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<(Entity, &mut Visibility, Has<Mesh3d>), With<LocalPlayerMarker>>,
) {
    // Always check and update, not just when changed, to ensure it's correct
    for (_entity, mut visibility, _has_mesh) in &mut local_player_query {
        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };

        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }
}

// Update player Transform from Position component for rendering
pub fn players_transform_sync_system(
    mut player_query: Query<(&Position, &mut Transform, Option<&CuboidShake>), With<PlayerMarker>>,
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
pub fn placers_face_to_transform_system(mut query: Query<(&FaceDirection, &mut Transform), Without<Camera3d>>) {
    for (face_dir, mut transform) in &mut query {
        transform.rotation = Quat::from_rotation_y(face_dir.0);
    }
}

// ============================================================================
// Players Billboard System
// ============================================================================

// Update rearview camera to look backwards from local player
pub fn local_player_rearview_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    main_camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<RearviewCameraMarker>)>,
    mut rearview_query: Query<&mut Transform, (With<RearviewCameraMarker>, Without<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok(mut rearview_transform) = rearview_query.single_mut() else {
        return;
    };

    // Only update in first-person view mode
    if *view_mode == CameraViewMode::FirstPerson {
        rearview_transform.translation.x = player_pos.x;
        rearview_transform.translation.z = player_pos.z;
        rearview_transform.translation.y = PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO;

        // Get the main camera's rotation and rotate 180 degrees
        if let Ok(main_transform) = main_camera_query.single() {
            let main_yaw = main_transform.rotation.to_euler(EulerRot::YXZ).0;
            let backwards_yaw = main_yaw + std::f32::consts::PI;
            rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
        }
    }
}

// Update rearview camera viewport based on window size
pub fn local_player_rearview_system(
    windows: Query<&Window>,
    mut rearview_query: Query<&mut Camera, With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut camera) = rearview_query.single_mut() else {
        return;
    };

    // Only show rearview in first-person mode
    let is_active = *view_mode == CameraViewMode::FirstPerson;
    camera.is_active = is_active;

    if !is_active {
        return;
    }

    let window_width = window.physical_width();
    let window_height = window.physical_height();

    let viewport_width = (window_width as f32 * REARVIEW_WIDTH_RATIO) as u32;
    let viewport_height = (window_height as f32 * REARVIEW_HEIGHT_RATIO) as u32;

    let margin_x = (window_width as f32 * REARVIEW_MARGIN) as u32;
    let margin_y = (window_height as f32 * REARVIEW_MARGIN) as u32;

    // Position in lower-right corner
    let x = window_width.saturating_sub(viewport_width + margin_x);
    let y = margin_y;

    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::new(viewport_width, viewport_height),
        depth: 0.0..1.0,
    });
}

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
