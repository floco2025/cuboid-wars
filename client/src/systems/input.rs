#[allow(clippy::wildcard_imports)]
use bevy::{math::Vec2, prelude::*};

use super::movement::LocalPlayer;
#[allow(clippy::wildcard_imports)]
use crate::constants::*;
use crate::{
    net::ClientToServer,
    resources::{CameraViewMode, ClientToServerChannel},
    spawning::spawn_projectile_local,
};
use common::protocol::{CFace, CShot, CSpeed, ClientMessage, FaceDirection, Position, Speed, SpeedLevel, Velocity};

// ============================================================================
// Input Systems
// ============================================================================

// Toggle camera view mode with V key
pub fn camera_view_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut view_mode: ResMut<CameraViewMode>) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        *view_mode = match *view_mode {
            CameraViewMode::FirstPerson => {
                info!("Switching to TopDown view");
                CameraViewMode::TopDown
            }
            CameraViewMode::TopDown => {
                info!("Switching to FirstPerson view");
                CameraViewMode::FirstPerson
            }
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
    mut last_sent_speed: Local<Speed>,    // Last speed sent to server
    mut last_sent_face: Local<f32>,       // Last face direction sent to server
    mut last_send_speed_time: Local<f32>, // Time accumulator for send speed interval throttling
    mut last_send_face_time: Local<f32>,  // Time accumulator for send face interval throttling
    mut stored_yaw: Local<f32>,           // Track player yaw across frames
    mut local_player_query: Query<(&mut Velocity, &mut FaceDirection), With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    // Require locked cursor before processing movement input
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;
    if !cursor_locked {
        // Drain pending mouse events and ensure player stops moving
        for _ in mouse_motion.read() {}

        if last_sent_speed.speed_level != SpeedLevel::Idle {
            let speed = Speed {
                speed_level: SpeedLevel::Idle,
                move_dir: 0.0,
            };
            for (mut player_velocity, _) in local_player_query.iter_mut() {
                *player_velocity = speed.to_velocity();
            }
            let msg = ClientMessage::Speed(CSpeed { speed });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_speed = speed;
            *last_send_speed_time = 0.0;
        }

        return;
    }

    // Determine the yaw baseline (camera vs stored value depending on view mode)
    let mut current_yaw = *stored_yaw;
    if *view_mode == CameraViewMode::FirstPerson && !view_mode.is_changed() {
        if let Some(transform) = camera_query.iter().next() {
            current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        }
    }

    // Apply mouse delta to yaw
    for motion in mouse_motion.read() {
        current_yaw -= motion.delta.x * MOUSE_SENSITIVITY;
    }

    *stored_yaw = current_yaw;
    let face_yaw = current_yaw + std::f32::consts::PI;

    // Build movement input vector (forward=z, right=x)
    let mut move_input = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        move_input.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        move_input.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        move_input.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        move_input.x -= 1.0;
    }

    // Translate input vector into move_dir + speed level
    let (speed_level, move_dir) = if move_input.length_squared() > 0.0 {
        let normalized_input = move_input.normalize();
        let angle_offset = normalized_input.x.atan2(normalized_input.y);
        let move_dir = face_yaw + angle_offset;
        let speed_level = if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            SpeedLevel::Run
        } else {
            SpeedLevel::Walk
        };
        (speed_level, move_dir)
    } else {
        (SpeedLevel::Idle, 0.0)
    };

    let speed = Speed { speed_level, move_dir };
    for (mut player_velocity, mut player_face) in local_player_query.iter_mut() {
        *player_velocity = speed.to_velocity();
        player_face.0 = face_yaw;
    }

    // Throttle network updates when movement/face changes
    let delta = time.delta_secs();
    *last_send_speed_time += delta;
    *last_send_face_time += delta;

    let speed_level_changed = last_sent_speed.speed_level != speed.speed_level;
    let move_dir_changed = (speed.move_dir - last_sent_speed.move_dir).abs() > SPEED_DIR_CHANGE_THRESHOLD;
    if speed_level_changed || (move_dir_changed && *last_send_speed_time >= SPEED_MAX_SEND_INTERVAL) {
        let msg = ClientMessage::Speed(CSpeed { speed });
        let _ = to_server.send(ClientToServer::Send(msg));
        *last_sent_speed = speed;
        *last_send_speed_time = 0.0;
    }

    let face_changed = (face_yaw - *last_sent_face).abs() > FACE_CHANGE_THRESHOLD;
    if face_changed && *last_send_face_time >= FACE_MAX_SEND_INTERVAL {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        *last_sent_face = face_yaw;
        *last_send_face_time = 0.0;
    }

    if *view_mode == CameraViewMode::FirstPerson {
        for mut transform in camera_query.iter_mut() {
            transform.rotation = Quat::from_rotation_y(current_yaw);
        }
    }
}

pub fn shooting_input_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    local_player_query: Query<(&Position, &FaceDirection), With<LocalPlayer>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    if cursor_locked && mouse.just_pressed(bevy::input::mouse::MouseButton::Left) {
        if let Some((pos, face_dir)) = local_player_query.iter().next() {
            // Play shooting sound
            commands.spawn((
                AudioPlayer::new(asset_server.load("sounds/player_fires.ogg")),
                PlaybackSettings::DESPAWN,
            ));

            // Send shot message with current face direction to server
            let shot_msg = ClientMessage::Shot(CShot { face_dir: face_dir.0 });
            let _ = to_server.send(ClientToServer::Send(shot_msg));

            // Spawn projectile locally
            spawn_projectile_local(&mut commands, &mut meshes, &mut materials, pos, face_dir.0);
        }
    }
}
