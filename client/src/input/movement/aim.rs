use bevy::{input::mouse::MouseMotion, prelude::*};

use crate::{
    cameras::{CameraViewMode, MainCameraMarker, TopDownCameraYaw},
    constants::MOUSE_SENSITIVITY,
    players::LocalPlayerInfo,
};

const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;

pub(super) fn calculate_current_orientation(
    mouse_motion: &mut MessageReader<MouseMotion>,
    camera_query: &Query<&mut Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    view_mode: &Res<CameraViewMode>,
    local_player_info: &mut LocalPlayerInfo,
    top_down_camera_yaw: &mut TopDownCameraYaw,
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
