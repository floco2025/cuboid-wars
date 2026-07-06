use bevy::{camera::Viewport, prelude::*, ui::UiScale};

use crate::{
    cameras::{CameraViewMode, MainCameraMarker, RearviewCameraMarker},
    characters::PreviousTickPosition,
    config::ClientSettings,
    constants::HUD_EDGE_MARGIN_PX,
    players::LocalPlayerMarker,
};
use common::{config::GameplayConfig, protocol::Position};

// Update rearview camera to look backwards from local player. Interpolates
// between the last and current fixed-tick positions like the main camera.
pub fn local_player_rearview_sync_system(
    local_player_query: Query<(&Position, &PreviousTickPosition), With<LocalPlayerMarker>>,
    main_camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<RearviewCameraMarker>)>,
    mut rearview_query: Query<&mut Transform, (With<RearviewCameraMarker>, Without<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
    fixed_time: Res<Time<Fixed>>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    if !client_settings.camera.rearview.enabled || !view_mode.is_first_person() {
        return;
    }

    let Some((current_pos, prev_pos)) = local_player_query.iter().next() else {
        return;
    };

    let Ok(mut rearview_transform) = rearview_query.single_mut() else {
        return;
    };

    let interp = prev_pos.lerp_to(*current_pos, fixed_time.overstep_fraction());
    rearview_transform.translation.x = interp.x;
    rearview_transform.translation.z = interp.z;
    rearview_transform.translation.y = interp.y + gameplay_config.player.eye_height();

    // Get the main camera's rotation and rotate 180 degrees.
    if let Ok(main_transform) = main_camera_query.single() {
        let main_yaw = main_transform.rotation.to_euler(EulerRot::YXZ).0;
        let backwards_yaw = main_yaw + std::f32::consts::PI;
        rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
    }
}

// Update rearview camera viewport based on window size.
pub fn local_player_rearview_viewport_system(
    windows: Query<&Window>,
    mut rearview_query: Query<&mut Camera, With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
    client_settings: Res<ClientSettings>,
    ui_scale: Res<UiScale>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut camera) = rearview_query.single_mut() else {
        return;
    };

    let is_active = client_settings.camera.rearview.enabled && view_mode.is_first_person();
    if camera.is_active != is_active {
        camera.is_active = is_active;
    }

    if !is_active {
        return;
    }

    let window_width = window.physical_width();
    let window_height = window.physical_height();

    let rearview = client_settings.camera.rearview;
    let viewport_width = (window_width as f32 * rearview.width_ratio) as u32;
    let viewport_height = (window_height as f32 * rearview.height_ratio) as u32;

    // Same inset as the HUD panels. Those are logical-pixel UI nodes scaled
    // by the HUD `UiScale`; the viewport is physical pixels, so apply both
    // the window scale factor and the HUD scale to keep the edges aligned.
    let margin = (HUD_EDGE_MARGIN_PX * window.scale_factor() * ui_scale.0) as u32;

    let x = window_width.saturating_sub(viewport_width + margin);
    let y = margin;

    let physical_position = UVec2::new(x, y);
    let physical_size = UVec2::new(viewport_width, viewport_height);
    // Reassigning an identical viewport every frame would mark the `Camera`
    // changed and churn render extraction; only write on a real change.
    let viewport_changed = camera
        .viewport
        .as_ref()
        .is_none_or(|v| v.physical_position != physical_position || v.physical_size != physical_size);
    if viewport_changed {
        camera.viewport = Some(Viewport {
            physical_position,
            physical_size,
            depth: 0.0..1.0,
        });
    }
}
