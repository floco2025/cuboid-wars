use bevy::{camera::Viewport, prelude::*};

use crate::{constants::*, markers::*};
use common::constants::{PLAYER_EYE_HEIGHT_RATIO, PLAYER_HEIGHT};

// ============================================================================
// Camera Setup System
// ============================================================================

pub fn setup_cameras_system(mut commands: Commands) {
    // Add main camera (initial position will be immediately overridden by sync system)
    commands.spawn((
        IsDefaultUiCamera, // Mark this as the UI camera
        MainCameraMarker,
        Camera3d::default(),
        Camera {
            // Render first to full window
            order: 0,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: FPV_CAMERA_FOV_DEGREES.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO, 0.0)
            .looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
    ));

    // Add rearview mirror camera (renders to lower-right viewport)
    commands.spawn((
        RearviewCameraMarker,
        Camera3d::default(),
        Camera {
            // Render after main camera to its viewport only
            order: 1,
            // Viewport will be set by rearview_camera_viewport_system
            viewport: Some(Viewport {
                physical_position: UVec2::ZERO,
                physical_size: UVec2::new(100, 100),
                depth: 0.0..1.0,
            }),
            // Don't clear the viewport - render on top
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: REARVIEW_FOV_DEGREES.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO, 0.0)
            .looking_at(Vec3::new(0.0, 0.0, 1.0), Vec3::Y), // Looking backwards (positive Z)
    ));
}
