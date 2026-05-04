use bevy::prelude::*;

use super::top_down::{topdown_camera_transform, window_aspect_ratio};
use crate::{
    cameras::{CameraViewMode, MainCameraMarker, TopDownCameraYaw},
    config::RenderSettings,
    players::{CameraShake, LocalPlayerMarker},
};
use common::{
    config::GameplayConfig,
    protocol::{MapLayout, Position},
};

// Update camera position to follow local player.
pub fn local_player_camera_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    map_layout: Option<Res<MapLayout>>,
    windows: Query<&Window>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    view_mode: Res<CameraViewMode>,
    top_down_camera_yaw: Res<TopDownCameraYaw>,
    render_settings: Res<RenderSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok((mut camera_transform, mut projection, maybe_shake)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection.as_mut() else {
        return;
    };

    match *view_mode {
        CameraViewMode::FirstPerson => {
            persp.fov = render_settings.fov_first_person_degrees.to_radians();
            sync_first_person_camera(
                &mut camera_transform,
                player_pos,
                gameplay_config.player.eye_height(),
                maybe_shake,
            );
        }
        CameraViewMode::TopDown => {
            persp.fov = render_settings.fov_top_down_degrees.to_radians();
            *camera_transform = topdown_camera_transform(
                player_pos,
                map_layout.as_deref(),
                window_aspect_ratio(&windows),
                persp.fov,
                top_down_camera_yaw.0,
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
