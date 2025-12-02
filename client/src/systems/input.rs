use bevy::{math::Vec2, prelude::*};

use super::players::LocalPlayer;
use crate::constants::*;
use crate::{
    net::ClientToServer,
    resources::{CameraViewMode, ClientToServerChannel, MyPlayerId, PlayerMap, RoofRenderingEnabled},
    spawning::spawn_projectiles_local,
};
use common::constants::SPEED_POWER_UP_MULTIPLIER;
use common::protocol::{CFace, CShot, CSpeed, ClientMessage, FaceDirection, Position, Speed, SpeedLevel, Velocity};

// ============================================================================
// Input Helpers
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

// Toggle roof rendering with R key
pub fn roof_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut roof_enabled: ResMut<RoofRenderingEnabled>) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        roof_enabled.0 = !roof_enabled.0;
        info!(
            "Roof rendering: {}",
            if roof_enabled.0 { "enabled" } else { "disabled" }
        );
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

// ============================================================================
// Input Systems
// ============================================================================

#[derive(Default)]
pub struct InputState {
    last_sent_speed: Speed,
    last_sent_face: f32,
    last_send_speed_time: f32,
    last_send_face_time: f32,
    stored_yaw: f32,
}

// Handle WASD movement and mouse rotation
pub fn input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    time: Res<Time>,
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
    mut local_state: Local<InputState>,
    mut local_player_query: Query<(&mut Velocity, &mut FaceDirection), With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    view_mode: Res<CameraViewMode>,
) {
    // Require locked cursor before processing movement input
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;
    if !cursor_locked {
        // Drain pending mouse events and ensure player stops moving
        for _ in mouse_motion.read() {}

        if local_state.last_sent_speed.speed_level != SpeedLevel::Idle {
            let speed = Speed {
                speed_level: SpeedLevel::Idle,
                move_dir: 0.0,
            };
            for (mut player_velocity, _) in &mut local_player_query {
                let mut velocity = speed.to_velocity();
                // Apply speed multiplier if local player has speed power-up
                if let Some(my_id) = my_player_id.as_ref()
                    && let Some(player_info) = players.0.get(&my_id.0)
                    && player_info.speed_power_up
                {
                    velocity.x *= SPEED_POWER_UP_MULTIPLIER;
                    velocity.z *= SPEED_POWER_UP_MULTIPLIER;
                }

                *player_velocity = velocity;
            }
            let msg = ClientMessage::Speed(CSpeed { speed });
            let _ = to_server.send(ClientToServer::Send(msg));
            local_state.last_sent_speed = speed;
            local_state.last_send_speed_time = 0.0;
        }

        return;
    }

    // Determine the yaw baseline (camera vs stored value depending on view mode)
    let current_yaw = if *view_mode == CameraViewMode::FirstPerson
        && !view_mode.is_changed()
        && let Some(transform) = camera_query.iter().next()
    {
        transform.rotation.to_euler(EulerRot::YXZ).0
    } else {
        local_state.stored_yaw
    };

    // Apply mouse delta to yaw
    let current_yaw = mouse_motion.read().fold(current_yaw, |yaw, motion| {
        motion.delta.x.mul_add(-MOUSE_SENSITIVITY, yaw)
    });

    local_state.stored_yaw = current_yaw;
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
    for (mut player_velocity, mut player_face) in &mut local_player_query {
        let mut velocity = speed.to_velocity();
        // Apply speed multiplier if local player has speed power-up
        if let Some(my_id) = my_player_id.as_ref()
            && let Some(player_info) = players.0.get(&my_id.0)
            && player_info.speed_power_up
        {
            velocity.x *= SPEED_POWER_UP_MULTIPLIER;
            velocity.z *= SPEED_POWER_UP_MULTIPLIER;
        }

        *player_velocity = velocity;
        player_face.0 = face_yaw;
    }

    // Throttle network updates when movement/face changes
    let delta = time.delta_secs();
    local_state.last_send_speed_time += delta;
    local_state.last_send_face_time += delta;

    let speed_level_changed = local_state.last_sent_speed.speed_level != speed.speed_level;
    let move_dir_changed =
        (speed.move_dir - local_state.last_sent_speed.move_dir).abs() > SPEED_DIR_CHANGE_THRESHOLD.to_radians();
    if speed_level_changed || (move_dir_changed && local_state.last_send_speed_time >= SPEED_MAX_SEND_INTERVAL) {
        let msg = ClientMessage::Speed(CSpeed { speed });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_state.last_sent_speed = speed;
        local_state.last_send_speed_time = 0.0;
    }

    let face_changed = (face_yaw - local_state.last_sent_face).abs() > FACE_CHANGE_THRESHOLD.to_radians();
    if face_changed && local_state.last_send_face_time >= FACE_MAX_SEND_INTERVAL {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_state.last_sent_face = face_yaw;
        local_state.last_send_face_time = 0.0;
    }

    if *view_mode == CameraViewMode::FirstPerson {
        for mut transform in &mut camera_query {
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
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
) {
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    if cursor_locked
        && mouse.just_pressed(bevy::input::mouse::MouseButton::Left)
        && let Some((pos, face_dir)) = local_player_query.iter().next()
    {
        // Play shooting sound
        commands.spawn((
            AudioPlayer::new(asset_server.load("sounds/player_fires.ogg")),
            PlaybackSettings::DESPAWN,
        ));

        // Send shot message with current face direction to server
        let shot_msg = ClientMessage::Shot(CShot { face_dir: face_dir.0 });
        let _ = to_server.send(ClientToServer::Send(shot_msg));

        // Check if player has multi-shot power-up
        let has_multi_shot = my_player_id
            .as_ref()
            .and_then(|id| players.0.get(&id.0))
            .is_some_and(|info| info.multi_shot_power_up);

        // Spawn projectile(s) based on power-up status
        spawn_projectiles_local(
            &mut commands,
            &mut meshes,
            &mut materials,
            pos,
            face_dir.0,
            has_multi_shot,
        );
    }
}
