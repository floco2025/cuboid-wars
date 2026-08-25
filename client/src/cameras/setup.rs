use bevy::{
    camera::{ImageRenderTarget, RenderTarget, Viewport},
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
    render::view::ColorGrading,
    window::PrimaryWindow,
};
use common::config::GameplayConfig;

use super::{
    CompositorCameraMarker, MainCameraMarker, RearviewCameraMarker, SceneRenderTarget, scene_target::create_scene_image,
};
use crate::config::ClientSettings;

// ============================================================================
// Camera Setup System
// ============================================================================

pub fn setup_cameras_system(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
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

    // The 3D cameras render into this image; `scene_render_target_system`
    // keeps its size in step with the window and the render-resolution cap.
    let window_size = windows
        .single()
        .map(|window| UVec2::new(window.physical_width().max(1), window.physical_height().max(1)))
        .unwrap_or(UVec2::new(1280, 720));
    let scene_image = create_scene_image(&mut images, window_size);
    commands.insert_resource(SceneRenderTarget {
        handle: scene_image.clone(),
        size: window_size,
    });

    // Add main camera (initial position will be immediately overridden by sync system)
    let mut main_camera = commands.spawn((
        MainCameraMarker,
        RenderTarget::Image(ImageRenderTarget {
            handle: scene_image.clone(),
            scale_factor: 1.0,
        }),
        // Ear pair for spatial audio emitters (explosion sounds): distance
        // attenuation + stereo panning relative to the camera.
        SpatialListener::new(0.3),
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
        // Present so lighting can drive `post_saturation` (low light mutes
        // the scene); defaults are a no-op grade.
        ColorGrading::default(),
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

    // Add rearview mirror camera (renders to its viewport inside the scene image)
    let mut rearview_camera = commands.spawn((
        RearviewCameraMarker,
        RenderTarget::Image(ImageRenderTarget {
            handle: scene_image.clone(),
            scale_factor: 1.0,
        }),
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
        ColorGrading::default(),
        Transform::from_xyz(0.0, player_eye_height, 0.0).looking_at(Vec3::new(0.0, 0.0, 1.0), Vec3::Y), // Looking backwards (positive Z)
    ));
    if deferred_rendering_enabled {
        rearview_camera.insert((DepthPrepass, DeferredPrepass));
    }

    // Compositor: shows the scene image upscaled to the window, then draws
    // the HUD (it is the default UI camera) at native resolution on top.
    commands.spawn((
        CompositorCameraMarker,
        IsDefaultUiCamera,
        Camera2d,
        Camera { order: 2, ..default() },
        Msaa::Off,
    ));
    // The scene displays as a UI image so it rides the HUD's existing UI
    // pass — a world `Sprite` adds a 2D scene phase costing ~2ms/frame.
    // The negative z-index keeps it under every HUD node (an extreme
    // sentinel like `i32::MIN` breaks the UI stack sort and draws nothing).
    commands.spawn((
        ImageNode {
            image: scene_image,
            image_mode: bevy::ui::widget::NodeImageMode::Stretch,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        GlobalZIndex(-1),
    ));
}
