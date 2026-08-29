use bevy::prelude::*;

use super::top_down::{topdown_camera_transform, window_aspect_ratio};
use crate::{
    cameras::{CameraViewMode, MainCameraMarker, TopDownCameraYaw},
    characters::PreviousTickPosition,
    config::ClientSettings,
    players::{CameraShake, LocalPlayerMarker},
};
use common::{
    config::GameplayConfig,
    protocol::{MapLayout, Position},
};

// Update camera position to follow local player. Physics ticks at 30 Hz;
// interpolate between last-tick and current-tick positions so the camera
// stays smooth at the render rate.
pub fn local_player_camera_sync_system(
    local_player_query: Query<(&Position, &PreviousTickPosition), With<LocalPlayerMarker>>,
    map_layout: Option<Res<MapLayout>>,
    windows: Query<&Window>,
    fixed_time: Res<Time<Fixed>>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    view_mode: Res<CameraViewMode>,
    top_down_camera_yaw: Res<TopDownCameraYaw>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    let Some((current_pos, prev_pos)) = local_player_query.iter().next() else {
        return;
    };
    let interp = prev_pos.lerp_to(*current_pos, fixed_time.overstep_fraction());
    let interpolated = Position {
        x: interp.x,
        y: interp.y,
        z: interp.z,
    };
    let player_pos = &interpolated;

    let Ok((mut camera_transform, mut projection, maybe_shake)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection.as_mut() else {
        return;
    };

    match *view_mode {
        CameraViewMode::FirstPerson => {
            persp.fov = client_settings.camera.fov_degrees.first_person.to_radians();
            sync_first_person_camera(
                &mut camera_transform,
                player_pos,
                gameplay_config.player.eye_height(),
                maybe_shake,
            );
        }
        CameraViewMode::TopDown => {
            persp.fov = client_settings.camera.fov_degrees.top_down.to_radians();
            *camera_transform = topdown_camera_transform(
                player_pos,
                map_layout.as_deref(),
                window_aspect_ratio(&windows),
                persp.fov,
                top_down_camera_yaw.0,
                client_settings.camera.topdown_margin,
                client_settings.camera.topdown_tilt_degrees,
            );
        }
    }
}

fn sync_first_person_camera(
    camera_transform: &mut Transform,
    player_pos: &Position,
    player_eye_height: f32,
    maybe_shake: Option<&CameraShake>,
) {
    camera_transform.translation.x = player_pos.x;
    camera_transform.translation.z = player_pos.z;
    camera_transform.translation.y = player_pos.y + player_eye_height;

    if let Some(shake) = maybe_shake {
        camera_transform.translation.x += shake.offset_x;
        camera_transform.translation.y += shake.offset_y;
        camera_transform.translation.z += shake.offset_z;
    }
}
