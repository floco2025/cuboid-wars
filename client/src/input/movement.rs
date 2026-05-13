use bevy::{
    input::mouse::MouseMotion,
    math::Vec2,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld, try_start_player_jump},
    protocol::{CJump, ClientMessage, FaceDirection, PlayerMoveIntent, Position},
};

use crate::{
    cameras::{CameraViewMode, MainCameraMarker, TopDownCameraYaw},
    config::ClientSettings,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};

const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;

type LocalPlayerInputQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static mut PlayerMoveIntent,
        &'static mut FaceDirection,
        &'static mut CharacterVerticalVelocity,
    ),
    With<LocalPlayerMarker>,
>;

// Handle WASD movement and mouse rotation at render rate. Writes
// `PlayerMoveIntent` and `FaceDirection` to the local-player ECS components
// continuously so the camera and local prediction stay smooth; the network
// commit happens once per game tick in `commit_player_input_system`. Jumps
// are sent immediately on key-press — discrete events feel best with no
// commit-tick latency.
pub fn input_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    cursor_options: Single<&CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    mut top_down_camera_yaw: ResMut<TopDownCameraYaw>,
    mut local_player_query: LocalPlayerInputQuery,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
    client_settings: Res<ClientSettings>,
) {
    let mouse_sensitivity = client_settings.input.mouse_sensitivity;
    // Wait for the local player entity to exist before sampling input.
    // Otherwise we'd compute a face direction from the default camera
    // transform and write it to ECS, overwriting the authoritative spawn-time
    // facing.
    if local_player_query.is_empty() {
        for _ in mouse_motion.read() {}
        return;
    }

    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;
    if !cursor_locked {
        // Drain mouse events and force idle intent locally; the commit
        // system will pick it up at the next tick boundary.
        for _ in mouse_motion.read() {}
        for (_, mut input, _, _) in local_player_query.iter_mut() {
            *input = PlayerMoveIntent::Idle;
        }
        return;
    }

    let (current_yaw, current_pitch) = calculate_current_orientation(
        &mut mouse_motion,
        &camera_query,
        &view_mode,
        &mut local_player_info,
        &mut top_down_camera_yaw,
        mouse_sensitivity,
    );
    let face_yaw = current_yaw + std::f32::consts::PI;
    // Death disables movement and jump just like stunned (and overrides it).
    let movement_disabled = local_player_info.is_dead || local_player_stunned(my_player_id.as_ref(), &players);
    let move_intent = calculate_move_intent(&keyboard, face_yaw, movement_disabled);
    let jump_requested = !movement_disabled && keyboard.just_pressed(KeyCode::Space);

    update_player_input_face_and_jump(
        move_intent,
        face_yaw,
        jump_requested,
        collision_world.as_deref(),
        &gameplay_config,
        &mut local_player_query,
    );

    // Jump is event-shaped, sent immediately. Move-intent and face are state,
    // sent by the per-tick commit system.
    if jump_requested {
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Jump(CJump {})));
    }

    if view_mode.is_first_person() {
        for mut transform in &mut camera_query {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, current_yaw, current_pitch, 0.0);
        }
    }
}

fn calculate_current_orientation(
    mouse_motion: &mut MessageReader<MouseMotion>,
    camera_query: &Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: &Res<CameraViewMode>,
    local_player_info: &mut LocalPlayerInfo,
    top_down_camera_yaw: &mut TopDownCameraYaw,
    mouse_sensitivity: f32,
) -> (f32, f32) {
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

    for motion in mouse_motion.read() {
        if view_mode.is_first_person() {
            current_yaw = motion.delta.x.mul_add(-mouse_sensitivity, current_yaw);
            current_pitch = motion.delta.y.mul_add(-mouse_sensitivity, current_pitch);
        } else {
            top_down_camera_yaw.0 = motion.delta.x.mul_add(-mouse_sensitivity, top_down_camera_yaw.0);
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

fn calculate_move_intent(keyboard: &Res<ButtonInput<KeyCode>>, face_yaw: f32, stunned: bool) -> PlayerMoveIntent {
    if stunned {
        return PlayerMoveIntent::Idle;
    }

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
        let direction = face_yaw + angle_offset;
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            PlayerMoveIntent::Running { direction }
        } else {
            PlayerMoveIntent::Walking { direction }
        }
    } else {
        PlayerMoveIntent::Idle
    }
}

fn local_player_stunned(my_player_id: Option<&Res<MyPlayerId>>, players: &Res<PlayerMap>) -> bool {
    my_player_id
        .and_then(|my_id| players.get(&my_id.0))
        .is_some_and(|player_info| player_info.stunned)
}

fn update_player_input_face_and_jump(
    move_intent: PlayerMoveIntent,
    face_yaw: f32,
    jump_requested: bool,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    local_player_query: &mut LocalPlayerInputQuery,
) {
    for (pos, mut input, mut face_direction, mut motion) in local_player_query.iter_mut() {
        *input = move_intent;
        face_direction.0 = face_yaw;
        if jump_requested && let Some(collision_world) = collision_world {
            let _ = try_start_player_jump(
                &mut motion.0,
                collision_world,
                gameplay_config.player.physics(),
                pos,
                pos.x,
                pos.z,
            );
        }
    }
}

