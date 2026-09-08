use bevy::{camera::Viewport, prelude::*, ui::UiScale};
use std::f32::consts::PI;

use crate::{
    cameras::{CameraViewMode, MainCameraMarker, RearviewCameraMarker, SceneRenderTarget},
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
    if !client_settings.preferences.rearview_mirror || !view_mode.is_first_person() {
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
        let backwards_yaw = main_yaw + PI;
        rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
    }
}

// Update rearview camera viewport based on the scene image size.
pub fn local_player_rearview_viewport_system(
    windows: Query<Ref<Window>>,
    mut rearview_query: Query<(&mut Camera, &mut Projection), With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
    client_settings: Res<ClientSettings>,
    ui_scale: Res<UiScale>,
    scene_target: Res<SceneRenderTarget>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    // These are the only inputs to the viewport and active state below.
    if !window.is_changed()
        && !view_mode.is_changed()
        && !client_settings.is_changed()
        && !ui_scale.is_changed()
        && !scene_target.is_changed()
    {
        return;
    }

    let Ok((mut camera, mut projection)) = rearview_query.single_mut() else {
        return;
    };

    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.fov = client_settings.preferences.fov_degrees.to_radians();
    }

    let is_active = client_settings.preferences.rearview_mirror && view_mode.is_first_person();
    if camera.is_active != is_active {
        camera.is_active = is_active;
    }

    if !is_active {
        return;
    }

    let target_size = scene_target.size;

    let rearview = client_settings.camera.rearview;
    let viewport_width = (target_size.x as f32 * rearview.width_ratio) as u32;
    let viewport_height = (target_size.y as f32 * rearview.height_ratio) as u32;

    // Same inset as the HUD panels. Those are logical-pixel UI nodes scaled
    // by the HUD `UiScale` in window space; the viewport lives in the scene
    // image, so additionally scale by image height over window height.
    let image_scale = target_size.y as f32 / window.physical_height().max(1) as f32;
    let margin = (HUD_EDGE_MARGIN_PX * window.scale_factor() * ui_scale.0 * image_scale) as u32;

    let x = target_size.x.saturating_sub(viewport_width + margin);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rearview_uses_live_follow_fov_independently_of_top_down() {
        let mut app = App::new();
        app.insert_resource(ClientSettings::load_default().expect("client settings are invalid"))
            .init_resource::<CameraViewMode>()
            .init_resource::<UiScale>()
            .insert_resource(SceneRenderTarget {
                handle: Handle::default(),
                size: UVec2::new(1200, 800),
            })
            .add_systems(Update, local_player_rearview_viewport_system);
        app.world_mut().spawn(Window::default());
        let camera = app
            .world_mut()
            .spawn((RearviewCameraMarker, Camera::default(), Projection::default()))
            .id();
        for fov in [75.0_f32, 105.0] {
            app.world_mut().resource_mut::<ClientSettings>().preferences.fov_degrees = fov;
            app.update();
            let Projection::Perspective(projection) = app
                .world()
                .get::<Projection>(camera)
                .expect("rearview projection missing")
            else {
                panic!("rearview projection is not perspective");
            };
            assert_eq!(projection.fov, fov.to_radians());
            assert_eq!(
                app.world().resource::<ClientSettings>().camera.top_down.fov_degrees,
                45.0
            );
        }
    }
}
