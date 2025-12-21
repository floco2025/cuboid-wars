use bevy::{camera::Viewport, prelude::*};

use super::components::CameraShake;
use crate::{constants::*, markers::*, resources::CameraViewMode};
use common::{
    constants::{PLAYER_EYE_HEIGHT_RATIO, PLAYER_HEIGHT},
    protocol::Position,
};

// ============================================================================
// Camera Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn local_player_camera_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    view_mode: Res<CameraViewMode>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    for (mut camera_transform, mut projection, maybe_shake) in &mut camera_query {
        match *view_mode {
            CameraViewMode::FirstPerson => {
                camera_transform.translation.x = player_pos.x;
                camera_transform.translation.z = player_pos.z;
                camera_transform.translation.y = PLAYER_HEIGHT.mul_add(PLAYER_EYE_HEIGHT_RATIO, player_pos.y);

                if let Some(shake) = maybe_shake {
                    camera_transform.translation.x += shake.offset_x;
                    camera_transform.translation.y += shake.offset_y;
                    camera_transform.translation.z += shake.offset_z;
                }

                // Set FPV FOV
                if let Projection::Perspective(persp) = projection.as_mut() {
                    persp.fov = FPV_CAMERA_FOV_DEGREES.to_radians();
                }
            }
            CameraViewMode::TopDown => {
                if view_mode.is_changed() {
                    camera_transform.translation = Vec3::new(0.0, TOPDOWN_CAMERA_HEIGHT, TOPDOWN_CAMERA_Z_OFFSET);
                }
                camera_transform.look_at(Vec3::new(TOPDOWN_LOOKAT_X, TOPDOWN_LOOKAT_Y, TOPDOWN_LOOKAT_Z), Vec3::Y);

                // Set top-down FOV
                if let Projection::Perspective(persp) = projection.as_mut() {
                    persp.fov = TOPDOWN_CAMERA_FOV_DEGREES.to_radians();
                }
            }
        }
    }
}

// Update local player visibility based on camera view mode
pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<(Entity, &mut Visibility, Has<Mesh3d>), With<LocalPlayerMarker>>,
) {
    // Always check and update, not just when changed, to ensure it's correct
    for (_entity, mut visibility, _has_mesh) in &mut local_player_query {
        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };

        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }
}

// Update rearview camera to look backwards from local player
pub fn local_player_rearview_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    main_camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<RearviewCameraMarker>)>,
    mut rearview_query: Query<&mut Transform, (With<RearviewCameraMarker>, Without<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok(mut rearview_transform) = rearview_query.single_mut() else {
        return;
    };

    // Only update in first-person view mode
    if *view_mode == CameraViewMode::FirstPerson {
        rearview_transform.translation.x = player_pos.x;
        rearview_transform.translation.z = player_pos.z;
        rearview_transform.translation.y = PLAYER_HEIGHT.mul_add(PLAYER_EYE_HEIGHT_RATIO, player_pos.y);

        // Get the main camera's rotation and rotate 180 degrees
        if let Ok(main_transform) = main_camera_query.single() {
            let main_yaw = main_transform.rotation.to_euler(EulerRot::YXZ).0;
            let backwards_yaw = main_yaw + std::f32::consts::PI;
            rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
        }
    }
}

// Update rearview camera viewport based on window size
pub fn local_player_rearview_system(
    windows: Query<&Window>,
    mut rearview_query: Query<&mut Camera, With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut camera) = rearview_query.single_mut() else {
        return;
    };

    // Only show rearview in first-person mode
    let is_active = *view_mode == CameraViewMode::FirstPerson;
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

    // Position in lower-right corner
    let x = window_width.saturating_sub(viewport_width + margin_x);
    let y = margin_y;

    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::new(viewport_width, viewport_height),
        depth: 0.0..1.0,
    });
}
