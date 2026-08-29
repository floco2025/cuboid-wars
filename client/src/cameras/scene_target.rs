use bevy::{
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureFormat},
    window::PrimaryWindow,
};

use super::{MainCameraMarker, RearviewCameraMarker, SceneRenderTarget};
use crate::config::ClientSettings;

pub fn create_scene_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    );
    // Pin linear filtering so the fullscreen upscale stays smooth even if the
    // global default sampler changes.
    image.sampler = ImageSampler::linear();
    images.add(image)
}

// Keep the scene image at the window size, capped at the configured render
// resolution.
pub fn scene_render_target_system(
    windows: Query<&Window, With<PrimaryWindow>>,
    client_settings: Res<ClientSettings>,
    mut scene_target: ResMut<SceneRenderTarget>,
    mut images: ResMut<Assets<Image>>,
    mut projections: Query<&mut Projection, Or<(With<MainCameraMarker>, With<RearviewCameraMarker>)>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = UVec2::new(window.physical_width(), window.physical_height());
    if window_size.x == 0 || window_size.y == 0 {
        return;
    }

    let desired = scene_image_size(window_size, client_settings.rendering.render_resolution);
    if desired != scene_target.size
        && let Some(mut image) = images.get_mut(&scene_target.handle)
    {
        image.resize(Extent3d {
            width: desired.x,
            height: desired.y,
            ..default()
        });
        scene_target.size = desired;
        // `camera_system` recomputes target info from `Assets<Image>` when a
        // projection changes; without this nudge it sees the resize one frame
        // late (asset events flush after it runs) and can extract a viewport
        // larger than the shrunken texture.
        for mut projection in &mut projections {
            projection.set_changed();
        }
    }
}

// Cap the height at the render resolution and follow the window's aspect
// ratio; a window smaller than the cap renders native.
fn scene_image_size(window_physical: UVec2, render_resolution: u32) -> UVec2 {
    if window_physical.y <= render_resolution {
        return window_physical;
    }
    let width = (window_physical.x as f32 * render_resolution as f32 / window_physical.y as f32).round() as u32;
    UVec2::new(width.max(1), render_resolution)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_window_caps_height_and_keeps_aspect() {
        assert_eq!(scene_image_size(UVec2::new(5120, 2880), 1440), UVec2::new(2560, 1440));
    }

    #[test]
    fn window_below_the_cap_renders_native() {
        assert_eq!(scene_image_size(UVec2::new(1920, 1080), 1440), UVec2::new(1920, 1080));
    }
}
