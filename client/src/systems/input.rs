use bevy::prelude::*;
use common::protocol::{CMovement, CShot, ClientMessage, Movement, Position, Velocity};

use super::sync::LocalPlayer;
use crate::{
    net::ClientToServer,
    resources::{ClientToServerChannel, CameraViewMode},
    spawning::spawn_projectile_local,
};


// ============================================================================
// Input Systems
// ============================================================================

// Toggle camera view mode with V key
pub fn camera_view_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut view_mode: ResMut<CameraViewMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        *view_mode = match *view_mode {
            CameraViewMode::FirstPerson => {
                info!("Switching to TopDown view");
                CameraViewMode::TopDown
            },
            CameraViewMode::TopDown => {
                info!("Switching to FirstPerson view");
                CameraViewMode::FirstPerson
            },
        };
    }
}

// Toggle cursor lock with Escape key or mouse click
pub fn cursor_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    mut cursor_options: Single<&mut bevy::window::CursorOptions>,
) {
    // Escape key toggles cursor lock
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor_options.visible = !cursor_options.visible;
        cursor_options.grab_mode = if cursor_options.visible {
            bevy::window::CursorGrabMode::None
        } else {
            bevy::window::CursorGrabMode::Locked
        };
    }

    // Left click locks cursor if it's currently unlocked
    // Don't consume the click - let it pass through to shooting system
    if mouse.just_pressed(bevy::input::mouse::MouseButton::Left) && cursor_options.visible {
        cursor_options.visible = false;
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        // Note: The click event will still be available for the shooting system
    }
}

// Handle WASD movement and mouse rotation
pub fn input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    time: Res<Time>,
    mut last_sent_movement: Local<Movement>, // Last movement sent to server
    mut last_send_time: Local<f32>,          // Time accumulator for send interval throttling
    mut player_rotation: Local<f32>,         // Track player rotation across frames
    mut local_player_query: Query<&mut Movement, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel
    const MOVEMENT_SEND_INTERVAL: f32 = 0.1; // Send movement updates at most every 100ms
    const ROTATION_CHANGE_THRESHOLD: f32 = 0.05; // ~3 degrees

    // Only process input when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    if cursor_locked {
        // Get current camera rotation (or player rotation in top-down mode)
        let mut camera_rotation = 0.0_f32;
        
        // When switching to FPV from top-down, use tracked rotation (not camera transform)
        // Otherwise in FPV, read from camera; in top-down, use tracked rotation
        if view_mode.is_changed() && *view_mode == CameraViewMode::FirstPerson {
            // Just switched to FPV - use the tracked rotation we maintained in top-down
            camera_rotation = *player_rotation;
        } else if *view_mode == CameraViewMode::FirstPerson {
            // Normal FPV operation - read from camera transform
            for transform in camera_query.iter() {
                camera_rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
            }
        } else {
            // In top-down, use the tracked rotation
            camera_rotation = *player_rotation;
        }

        // Handle mouse rotation
        for motion in mouse_motion.read() {
            camera_rotation -= motion.delta.x * MOUSE_SENSITIVITY;
        }
        
        // Always update tracked rotation (so it's current for next mode switch)
        *player_rotation = camera_rotation;

        // Get forward/right vectors from camera rotation
        // Camera rotation maps directly to face direction
        let face_dir = camera_rotation;

        // Handle WASD input relative to camera direction
        let mut forward = 0.0_f32;
        let mut right = 0.0_f32;

        if keyboard.pressed(KeyCode::KeyW) {
            forward -= 1.0; // Move forward
        }
        if keyboard.pressed(KeyCode::KeyS) {
            forward += 1.0; // Move backward
        }
        if keyboard.pressed(KeyCode::KeyA) {
            right -= 1.0; // Move left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            right += 1.0; // Move right
        }

        // Calculate movement direction
        let (vel_state, move_direction) = if forward != 0.0 || right != 0.0 {
            // Normalize the input vector
            let len = (forward * forward + right * right).sqrt();
            let norm_forward = forward / len;
            let norm_right = right / len;
            
            // Calculate angle offset from face direction
            // forward=1, right=0 -> offset=0 (moving in face direction)
            // forward=0, right=1 -> offset=π/2 (moving right)
            let angle_offset = norm_right.atan2(norm_forward);
            let move_dir = face_dir + angle_offset;
            // Check if shift is pressed for running
            let vel = if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                Velocity::Run
            } else {
                Velocity::Walk
            };
            (vel, move_dir)
        } else {
            // Idle - movement direction doesn't matter
            (Velocity::Idle, 0.0)
        };

        // Player faces camera direction
        // Add π because camera_rotation=0 points backwards from where we want face_dir=0
        let face_direction = camera_rotation + std::f32::consts::PI;

        // Create movement
        let mov = Movement {
            vel: vel_state,
            move_dir: move_direction,
            face_dir: face_direction,
        };

        // Always update local player's movement immediately for responsive local movement
        for mut player_mov in local_player_query.iter_mut() {
            *player_mov = mov;
        }

        // Accumulate send time for throttling
        *last_send_time += time.delta_secs();

        // Determine if movement changed significantly
        let vel_state_changed = last_sent_movement.vel != mov.vel;
        let rotation_changed = (mov.face_dir - last_sent_movement.face_dir).abs() > ROTATION_CHANGE_THRESHOLD;

        // Send to server if:
        // 1. Velocity state changed (started/stopped moving, or changed speed), OR
        // 2. Direction changed significantly AND enough time has passed (throttle minor direction updates)
        let should_send = vel_state_changed || (rotation_changed && *last_send_time >= MOVEMENT_SEND_INTERVAL);

        if should_send {
            let msg = ClientMessage::Movement(CMovement { mov });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_movement = mov;
            *last_send_time = 0.0;
        }

        // Update camera rotation
        for mut transform in camera_query.iter_mut() {
            // Only update rotation in first-person view
            // In top-down, preserve the look_at() rotation from sync system
            if *view_mode == CameraViewMode::FirstPerson {
                transform.rotation = Quat::from_rotation_y(camera_rotation);
            }
        }
    } else {
        // Cursor not locked - clear mouse motion events to prevent them from accumulating
        for _ in mouse_motion.read() {}

        // Get current camera rotation for setting idle direction
        let mut camera_rotation = 0.0_f32;
        for transform in camera_query.iter() {
            camera_rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
        }

        // Stop player movement when cursor is unlocked
        if last_sent_movement.vel != Velocity::Idle {
            let mov = Movement {
                vel: Velocity::Idle,
                move_dir: 0.0,
                face_dir: camera_rotation,
            };
            for mut player_mov in local_player_query.iter_mut() {
                *player_mov = mov;
            }
            let msg = ClientMessage::Movement(CMovement { mov });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_movement = mov;
            *last_send_time = 0.0;
        }
    }
}

pub fn shooting_input_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    local_player_query: Query<(&Position, &Movement), With<LocalPlayer>>,
    to_server: Res<ClientToServerChannel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;
    
    if cursor_locked && mouse.just_pressed(bevy::input::mouse::MouseButton::Left) {
        if let Some((pos, mov)) = local_player_query.iter().next() {
            // Send shot message with current movement to server
            let shot_msg = ClientMessage::Shot(CShot { mov: *mov });
            let _ = to_server.send(ClientToServer::Send(shot_msg));
            
            // Spawn projectile locally
            spawn_projectile_local(&mut commands, &mut meshes, &mut materials, pos, mov);
        }
    }
}
