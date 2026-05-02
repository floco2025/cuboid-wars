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
    resources::{CameraViewMode, ClientToServerChannel, LocalPlayerInfo, MyPlayerId, PlayerMap, TopDownCameraYaw},
};
use common::config::GameplayConfig;
use common::physics::{CharacterVerticalMotion, CollisionWorld, try_start_player_jump};
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
    mut local_player_info: ResMut<LocalPlayerInfo>,
    mut top_down_camera_yaw: ResMut<TopDownCameraYaw>,
    mut local_player_query: Query<
        (
            &Position,
            &mut CharacterMoveIntent,
            &mut FaceDirection,
            &mut CharacterVerticalMotion,
        ),
        With<LocalPlayerMarker>,
    >,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
) {
    // Wait for the local player entity to exist before sampling input or sending updates.
    // Otherwise we'd compute a face direction from the default camera transform and
    // broadcast it to the server, overwriting the authoritative spawn-time facing.
    if local_player_query.is_empty() {
        for _ in mouse_motion.read() {}
        return;
    }

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
        &mut top_down_camera_yaw,
    );
    let face_yaw = current_yaw + std::f32::consts::PI;
    let stunned = local_player_stunned(my_player_id.as_ref(), &players);
    let move_intent = calculate_move_intent(&keyboard, face_yaw, stunned);
    let jump_requested = !stunned && keyboard.just_pressed(KeyCode::Space);

    update_player_input_face_and_jump(
        move_intent,
        face_yaw,
        jump_requested,
        collision_world.as_deref(),
        &gameplay_config,
        &mut local_player_query,
    );

    send_throttled_updates(
        move_intent,
        face_yaw,
        jump_requested,
        &time,
        &to_server,
        &mut local_player_info,
    );

    if view_mode.is_first_person() {
        for mut transform in &mut camera_query {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, current_yaw, current_pitch, 0.0);
        }
    }
}

fn handle_unlocked_cursor(
    mouse_motion: &mut MessageReader<MouseMotion>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
    local_player_query: &mut Query<
        (
            &Position,
            &mut CharacterMoveIntent,
            &mut FaceDirection,
            &mut CharacterVerticalMotion,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    // Drain pending mouse events and ensure player stops moving
    for _ in mouse_motion.read() {}

    if local_player_info.last_sent_move_intent.direction().is_some() {
        let idle = CharacterMoveIntent::Idle;
        for (_, mut input, _, _) in local_player_query.iter_mut() {
            *input = idle;
        }
        let msg = ClientMessage::PlayerMoveIntent(CPlayerMoveIntent { move_intent: idle });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_move_intent = idle;
        local_player_info.last_send_input_time = 0.0;
    }
}

fn calculate_current_orientation(
    mouse_motion: &mut MessageReader<MouseMotion>,
    camera_query: &Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: &Res<CameraViewMode>,
    local_player_info: &mut LocalPlayerInfo,
    top_down_camera_yaw: &mut TopDownCameraYaw,
) -> (f32, f32) {
    // Determine the yaw/pitch baseline (camera vs stored value depending on view mode)
    let (mut current_yaw, mut current_pitch) = if view_mode.is_first_person() {
        if !view_mode.is_changed()
            && let Some(transform) = camera_query.iter().next()
        {
            let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
            (yaw, pitch)
        } else {
            (local_player_info.stored_yaw, local_player_info.stored_pitch)
        }
    } else {
        (top_down_camera_yaw.0, 0.0)
    };

    // Apply mouse delta to first-person aim or top-down camera rotation.
    for motion in mouse_motion.read() {
        if view_mode.is_first_person() {
            current_yaw = motion.delta.x.mul_add(-MOUSE_SENSITIVITY, current_yaw);
            current_pitch = motion.delta.y.mul_add(-MOUSE_SENSITIVITY, current_pitch);
        } else {
            top_down_camera_yaw.0 = motion.delta.x.mul_add(-MOUSE_SENSITIVITY, top_down_camera_yaw.0);
            current_yaw = top_down_camera_yaw.0;
        }
    }

    if view_mode.is_first_person() {
        current_pitch = current_pitch.clamp(-MAX_PITCH, MAX_PITCH);
    } else {
        current_pitch = 0.0;
    }

    local_player_info.stored_yaw = current_yaw;
    local_player_info.stored_pitch = current_pitch;
    (current_yaw, current_pitch)
}

fn calculate_move_intent(keyboard: &Res<ButtonInput<KeyCode>>, face_yaw: f32, stunned: bool) -> CharacterMoveIntent {
    // Stunned players cannot move
    if stunned {
        return CharacterMoveIntent::Idle;
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
        CharacterMoveIntent::Moving {
            direction: face_yaw + angle_offset,
        }
    } else {
        CharacterMoveIntent::Idle
    }
}

fn local_player_stunned(my_player_id: Option<&Res<MyPlayerId>>, players: &Res<PlayerMap>) -> bool {
    my_player_id
        .and_then(|my_id| players.0.get(&my_id.0))
        .is_some_and(|player_info| player_info.stunned)
}

fn update_player_input_face_and_jump(
    move_intent: CharacterMoveIntent,
    face_yaw: f32,
    jump_requested: bool,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    local_player_query: &mut Query<
        (
            &Position,
            &mut CharacterMoveIntent,
            &mut FaceDirection,
            &mut CharacterVerticalMotion,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    for (pos, mut input, mut face_direction, mut motion) in local_player_query.iter_mut() {
        *input = move_intent;
        face_direction.0 = face_yaw;
        if jump_requested && let Some(collision_world) = collision_world {
            let _ = try_start_player_jump(
                &mut motion.0,
                collision_world,
                gameplay_config.characters.player.physics(),
                pos,
                pos.x,
                pos.z,
            );
        }
    }
}

fn send_throttled_updates(
    move_intent: CharacterMoveIntent,
    face_yaw: f32,
    jump_requested: bool,
    time: &Res<Time>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
) {
    // Throttle network updates when movement/face changes
    let delta = time.delta_secs();
    local_player_info.last_send_input_time += delta;
    local_player_info.last_send_face_time += delta;

    let last_dir = local_player_info.last_sent_move_intent.direction();
    let new_dir = move_intent.direction();
    let active_changed = last_dir.is_some() != new_dir.is_some();
    let direction_changed = match (new_dir, last_dir) {
        (Some(new_d), Some(old_d)) => (new_d - old_d).abs() > MOVE_INPUT_DIR_CHANGE_THRESHOLD.to_radians(),
        _ => false,
    };
    if active_changed || (direction_changed && local_player_info.last_send_input_time >= MOVE_INPUT_MAX_SEND_INTERVAL) {
        let msg = ClientMessage::PlayerMoveIntent(CPlayerMoveIntent { move_intent });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_move_intent = move_intent;
        local_player_info.last_send_input_time = 0.0;
    }

    let face_changed = (face_yaw - local_player_info.last_sent_face).abs() > FACE_CHANGE_THRESHOLD.to_radians();
    if face_changed && local_player_info.last_send_face_time >= FACE_MAX_SEND_INTERVAL {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_face = face_yaw;
        local_player_info.last_send_face_time = 0.0;
    }

    if jump_requested {
        let msg = ClientMessage::Jump(CJump {});
        let _ = to_server.send(ClientToServer::Send(msg));
    }
}
