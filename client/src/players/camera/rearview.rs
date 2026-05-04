use bevy::{camera::Viewport, prelude::*};

use crate::{
    cameras::{CameraViewMode, MainCameraMarker, RearviewCameraMarker},
    config::RenderSettings,
    constants::{REARVIEW_HEIGHT_RATIO, REARVIEW_MARGIN, REARVIEW_WIDTH_RATIO},
    players::LocalPlayerMarker,
};
use common::{config::GameplayConfig, protocol::Position};

// Update rearview camera to look backwards from local player.
pub fn local_player_rearview_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    main_camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<RearviewCameraMarker>)>,
    mut rearview_query: Query<&mut Transform, (With<RearviewCameraMarker>, Without<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
    render_settings: Res<RenderSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    if !render_settings.rearview_enabled || !view_mode.is_first_person() {
        return;
    }

    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok(mut rearview_transform) = rearview_query.single_mut() else {
        return;
    };

    rearview_transform.translation.x = player_pos.x;
    rearview_transform.translation.z = player_pos.z;
    rearview_transform.translation.y = player_pos.y + gameplay_config.player.eye_height();

    // Get the main camera's rotation and rotate 180 degrees.
    if let Ok(main_transform) = main_camera_query.single() {
        let main_yaw = main_transform.rotation.to_euler(EulerRot::YXZ).0;
        let backwards_yaw = main_yaw + std::f32::consts::PI;
        rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
    }
}

// Update rearview camera viewport based on window size.
pub fn local_player_rearview_system(
    windows: Query<&Window>,
    mut rearview_query: Query<&mut Camera, With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
    render_settings: Res<RenderSettings>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut camera) = rearview_query.single_mut() else {
        return;
    };

    let is_active = render_settings.rearview_enabled && view_mode.is_first_person();
    camera.is_active = is_active;

    if !is_active {
        return;
    }

    let window_width = window.physical_width();
    let window_height = window.physical_height();

    let viewport_width = (window_width as f32 * REARVIEW_WIDTH_RATIO) as u32;
    let viewport_height = (window_height as f32 * REARVIEW_HEIGHT_RATIO) as u32;

    let margin_x = (window_width as f32 * REARVIEW_MARGIN) as u32;
    let margin_y = (window_height as f32 * REARVIEW_MARGIN) as u32;

    let x = window_width.saturating_sub(viewport_width + margin_x);
    let y = margin_y;

    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::new(viewport_width, viewport_height),
        depth: 0.0..1.0,
    });
}
