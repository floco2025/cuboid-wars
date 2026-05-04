use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use super::{
    aim::calculate_current_orientation,
    cursor::handle_unlocked_cursor,
    intent::{calculate_move_intent, local_player_stunned},
    jump::update_player_input_face_and_jump,
    network_commands::send_throttled_updates,
    types::LocalPlayerInputQuery,
};
use crate::{
    cameras::{CameraViewMode, MainCameraMarker, TopDownCameraYaw},
    network::ClientToServerChannel,
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
};
use common::{config::GameplayConfig, physics::CollisionWorld};

// Handle WASD movement and mouse rotation.
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
    mut local_player_query: LocalPlayerInputQuery,
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
