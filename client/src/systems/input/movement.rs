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
    resources::{
        CameraViewMode, ClientToServerChannel, InputSettings, LocalPlayerInfo, MyPlayerId, PlayerMap,
    },
    systems::players::PlayerMovementMut,
};
use common::{
    constants::{ALWAYS_SPEED, POWER_UP_SPEED_MULTIPLIER},
    protocol::*,
};

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
    mut local_player_query: Query<PlayerMovementMut, With<LocalPlayerMarker>>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
) {
    // Require locked cursor before processing movement input
    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;
    if !cursor_locked {
        handle_unlocked_cursor(
            &mut mouse_motion,
            &to_server,
            my_player_id.as_ref(),
            &players,
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
    let speed = calculate_movement_speed(&keyboard, face_yaw, my_player_id.as_ref(), &players);

    update_player_velocity_and_face(
        speed,
        face_yaw,
        my_player_id.as_ref(),
        &players,
        &mut local_player_query,
    );

    send_throttled_updates(speed, face_yaw, &time, &to_server, &mut local_player_info);

    if *view_mode == CameraViewMode::FirstPerson {
        for mut transform in &mut camera_query {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, current_yaw, current_pitch, 0.0);
        }
    }
}

fn handle_unlocked_cursor(
    mouse_motion: &mut MessageReader<MouseMotion>,
    to_server: &Res<ClientToServerChannel>,
    my_player_id: Option<&Res<MyPlayerId>>,
    players: &Res<PlayerMap>,
    local_player_info: &mut LocalPlayerInfo,
    local_player_query: &mut Query<PlayerMovementMut, With<LocalPlayerMarker>>,
) {
    // Drain pending mouse events and ensure player stops moving
    for _ in mouse_motion.read() {}

    if local_player_info.last_sent_speed.speed_level != SpeedLevel::Idle {
        let speed = Speed {
            speed_level: SpeedLevel::Idle,
            move_dir: 0.0,
        };
        for mut player in local_player_query {
            let mut velocity = speed.to_velocity();
            // Apply speed multiplier if local player has speed power-up
            if let Some(my_id) = my_player_id
                && let Some(player_info) = players.0.get(&my_id.0)
                && (ALWAYS_SPEED || player_info.speed_power_up)
            {
                velocity.x *= POWER_UP_SPEED_MULTIPLIER;
                velocity.z *= POWER_UP_SPEED_MULTIPLIER;
            }

            *player.velocity = velocity;
        }
        let msg = ClientMessage::Speed(CSpeed { speed });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_speed = speed;
        local_player_info.last_send_speed_time = 0.0;
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

fn calculate_movement_speed(
    keyboard: &Res<ButtonInput<KeyCode>>,
    face_yaw: f32,
    my_player_id: Option<&Res<MyPlayerId>>,
    players: &Res<PlayerMap>,
) -> Speed {
    // Check if stunned - if so, no movement
    if let Some(my_id) = my_player_id
        && let Some(player_info) = players.0.get(&my_id.0)
        && player_info.stunned
    {
        return Speed {
            speed_level: SpeedLevel::Idle,
            move_dir: 0.0,
        };
    }

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

    Speed { speed_level, move_dir }
}

fn update_player_velocity_and_face(
    speed: Speed,
    face_yaw: f32,
    my_player_id: Option<&Res<MyPlayerId>>,
    players: &Res<PlayerMap>,
    local_player_query: &mut Query<PlayerMovementMut, With<LocalPlayerMarker>>,
) {
    for mut player in local_player_query {
        let mut velocity = speed.to_velocity();
        // Apply speed multiplier if local player has speed power-up
        if let Some(my_id) = my_player_id
            && let Some(player_info) = players.0.get(&my_id.0)
            && (ALWAYS_SPEED || player_info.speed_power_up)
        {
            velocity.x *= POWER_UP_SPEED_MULTIPLIER;
            velocity.z *= POWER_UP_SPEED_MULTIPLIER;
        }

        *player.velocity = velocity;
        player.face_direction.0 = face_yaw;
    }
}

fn send_throttled_updates(
    speed: Speed,
    face_yaw: f32,
    time: &Res<Time>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
) {
    // Throttle network updates when movement/face changes
    let delta = time.delta_secs();
    local_player_info.last_send_speed_time += delta;
    local_player_info.last_send_face_time += delta;

    let speed_level_changed = local_player_info.last_sent_speed.speed_level != speed.speed_level;
    let move_dir_changed =
        (speed.move_dir - local_player_info.last_sent_speed.move_dir).abs() > SPEED_DIR_CHANGE_THRESHOLD.to_radians();
    if speed_level_changed || (move_dir_changed && local_player_info.last_send_speed_time >= SPEED_MAX_SEND_INTERVAL) {
        let msg = ClientMessage::Speed(CSpeed { speed });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_speed = speed;
        local_player_info.last_send_speed_time = 0.0;
    }

    let face_changed = (face_yaw - local_player_info.last_sent_face).abs() > FACE_CHANGE_THRESHOLD.to_radians();
    if face_changed && local_player_info.last_send_face_time >= FACE_MAX_SEND_INTERVAL {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_face = face_yaw;
        local_player_info.last_send_face_time = 0.0;
    }
}
