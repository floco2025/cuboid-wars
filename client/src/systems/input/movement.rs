use bevy::{
    input::mouse::MouseMotion,
    math::Vec2,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    constants::*,
    markers::{LocalPlayerMarker, MainCameraMarker},
    net::ClientToServer,
    resources::{CameraViewMode, ClientToServerChannel, InputSettings, LocalPlayerInfo, MyPlayerId, PlayerMap},
};
use common::protocol::*;

const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;

// Handle WASD movement and mouse rotation
pub fn input_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    cursor_options: Single<&CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    time: Res<Time>,
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
    input_settings: Res<InputSettings>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    mut local_player_query: Query<(&mut MoveInput, &mut FaceDirection), With<LocalPlayerMarker>>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
) {
    // Require locked cursor before processing movement input
    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;
    if !cursor_locked {
        handle_unlocked_cursor(
            &mut mouse_motion,
            &to_server,
            &mut local_player_info,
            &mut local_player_query,
        );
        return;
    }

    let (current_yaw, current_pitch) = calculate_current_orientation(
        &mut mouse_motion,
        &camera_query,
        &view_mode,
        &mut local_player_info,
        input_settings.invert_pitch,
    );
    let face_yaw = current_yaw + std::f32::consts::PI;
    let move_input = calculate_move_input(&keyboard, face_yaw, my_player_id.as_ref(), &players);

    update_player_input_and_face(move_input, face_yaw, &mut local_player_query);

    send_throttled_updates(move_input, face_yaw, &time, &to_server, &mut local_player_info);

    if *view_mode == CameraViewMode::FirstPerson {
        for mut transform in &mut camera_query {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, current_yaw, current_pitch, 0.0);
        }
    }
}

fn handle_unlocked_cursor(
    mouse_motion: &mut MessageReader<MouseMotion>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
    local_player_query: &mut Query<(&mut MoveInput, &mut FaceDirection), With<LocalPlayerMarker>>,
) {
    // Drain pending mouse events and ensure player stops moving
    for _ in mouse_motion.read() {}

    if local_player_info.last_sent_input.direction().is_some() {
        let idle = MoveInput::Idle;
        for (mut input, _) in local_player_query.iter_mut() {
            *input = idle;
        }
        let msg = ClientMessage::MoveInput(CMoveInput { move_input: idle });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_input = idle;
        local_player_info.last_send_input_time = 0.0;
    }
}

fn calculate_current_orientation(
    mouse_motion: &mut MessageReader<MouseMotion>,
    camera_query: &Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: &Res<CameraViewMode>,
    local_player_info: &mut LocalPlayerInfo,
    invert_pitch: bool,
) -> (f32, f32) {
    let pitch_sign = if invert_pitch {
        MOUSE_SENSITIVITY
    } else {
        -MOUSE_SENSITIVITY
    };
    // Determine the yaw/pitch baseline (camera vs stored value depending on view mode)
    let (mut current_yaw, mut current_pitch) = if **view_mode == CameraViewMode::FirstPerson
        && !view_mode.is_changed()
        && let Some(transform) = camera_query.iter().next()
    {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
        (yaw, pitch)
    } else {
        (local_player_info.stored_yaw, local_player_info.stored_pitch)
    };

    // Apply mouse delta to yaw/pitch (pitch only in first-person)
    for motion in mouse_motion.read() {
        current_yaw = motion.delta.x.mul_add(-MOUSE_SENSITIVITY, current_yaw);
        if **view_mode == CameraViewMode::FirstPerson {
            current_pitch = motion.delta.y.mul_add(pitch_sign, current_pitch);
        }
    }

    if **view_mode == CameraViewMode::FirstPerson {
        current_pitch = current_pitch.clamp(-MAX_PITCH, MAX_PITCH);
    } else {
        current_pitch = 0.0;
    }

    local_player_info.stored_yaw = current_yaw;
    local_player_info.stored_pitch = current_pitch;
    (current_yaw, current_pitch)
}

fn calculate_move_input(
    keyboard: &Res<ButtonInput<KeyCode>>,
    face_yaw: f32,
    my_player_id: Option<&Res<MyPlayerId>>,
    players: &Res<PlayerMap>,
) -> MoveInput {
    // Stunned players cannot move
    if let Some(my_id) = my_player_id
        && let Some(player_info) = players.0.get(&my_id.0)
        && player_info.stunned
    {
        return MoveInput::Idle;
    }

    // Build movement input vector (forward=z, right=x)
    let mut keyboard_vec = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        keyboard_vec.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        keyboard_vec.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        keyboard_vec.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        keyboard_vec.x -= 1.0;
    }

    if keyboard_vec.length_squared() > 0.0 {
        let normalized_input = keyboard_vec.normalize();
        let angle_offset = normalized_input.x.atan2(normalized_input.y);
        MoveInput::Moving {
            direction: face_yaw + angle_offset,
        }
    } else {
        MoveInput::Idle
    }
}

fn update_player_input_and_face(
    move_input: MoveInput,
    face_yaw: f32,
    local_player_query: &mut Query<(&mut MoveInput, &mut FaceDirection), With<LocalPlayerMarker>>,
) {
    for (mut input, mut face_direction) in local_player_query.iter_mut() {
        *input = move_input;
        face_direction.0 = face_yaw;
    }
}

fn send_throttled_updates(
    move_input: MoveInput,
    face_yaw: f32,
    time: &Res<Time>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
) {
    // Throttle network updates when movement/face changes
    let delta = time.delta_secs();
    local_player_info.last_send_input_time += delta;
    local_player_info.last_send_face_time += delta;

    let last_dir = local_player_info.last_sent_input.direction();
    let new_dir = move_input.direction();
    let active_changed = last_dir.is_some() != new_dir.is_some();
    let direction_changed = match (new_dir, last_dir) {
        (Some(new_d), Some(old_d)) => (new_d - old_d).abs() > MOVE_INPUT_DIR_CHANGE_THRESHOLD.to_radians(),
        _ => false,
    };
    if active_changed || (direction_changed && local_player_info.last_send_input_time >= MOVE_INPUT_MAX_SEND_INTERVAL) {
        let msg = ClientMessage::MoveInput(CMoveInput { move_input });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_input = move_input;
        local_player_info.last_send_input_time = 0.0;
    }

    let face_changed = (face_yaw - local_player_info.last_sent_face).abs() > FACE_CHANGE_THRESHOLD.to_radians();
    if face_changed && local_player_info.last_send_face_time >= FACE_MAX_SEND_INTERVAL {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_face = face_yaw;
        local_player_info.last_send_face_time = 0.0;
    }
}
