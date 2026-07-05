use bevy::{
    camera::Viewport,
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
};
use common::config::GameplayConfig;

use super::{MainCameraMarker, RearviewCameraMarker};
use crate::config::ClientSettings;

// ============================================================================
// Camera Setup System
// ============================================================================

pub fn setup_cameras_system(
    mut commands: Commands,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    let deferred_rendering_enabled = client_settings.rendering.opaque_renderer.is_deferred();
    let player_eye_height = gameplay_config.player.eye_height();
    let msaa = if deferred_rendering_enabled {
        Msaa::Off
    } else {
        Msaa::from_samples(client_settings.rendering.msaa_samples)
    };

    // Add main camera (initial position will be immediately overridden by sync system)
    let mut main_camera = commands.spawn((
        IsDefaultUiCamera, // Mark this as the UI camera
        MainCameraMarker,
        msaa,
        Camera3d::default(),
        Camera {
            // Render first to full window
            order: 0,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: client_settings.camera.fov_first_person_degrees.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, player_eye_height, 0.0).looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
    ));
    if deferred_rendering_enabled {
        main_camera.insert((DepthPrepass, DeferredPrepass));
    }
    let bloom = client_settings.rendering.bloom;
    if bloom.enabled {
        // Thresholded additive bloom (switches the camera to HDR): pixels
        // below the threshold are untouched — the energy-conserving mode
        // mixes the blur into the WHOLE image and desaturates the scene.
        // Only true HDR emitters (sun disc, projectiles, sparks) overglow.
        main_camera.insert(Bloom {
            intensity: bloom.intensity,
            prefilter: BloomPrefilter {
                threshold: bloom.threshold,
                threshold_softness: bloom.threshold_softness,
            },
            composite_mode: BloomCompositeMode::Additive,
            ..Bloom::NATURAL
        });
    }

    // Add rearview mirror camera (renders to lower-right viewport)
    let mut rearview_camera = commands.spawn((
        RearviewCameraMarker,
        msaa,
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
            is_active: client_settings.camera.rearview.enabled,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: client_settings.camera.rearview.fov_degrees.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, player_eye_height, 0.0).looking_at(Vec3::new(0.0, 0.0, 1.0), Vec3::Y), // Looking backwards (positive Z)
    ));
    if deferred_rendering_enabled {
        rearview_camera.insert((DepthPrepass, DeferredPrepass));
    }
}
