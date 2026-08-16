use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

// Generated silhouette masks for the player-list icons. UI nodes only draw
// rectangles (plus border radius), but the speed power-up is a tetrahedron
// in-game and a triangle in the editor — so its HUD icon is a white
// triangle alpha mask, tinted per state via `ImageNode.color`.
#[derive(Resource)]
pub struct HudShapeAssets {
    pub triangle: Handle<Image>,
}

const TRIANGLE_MASK_SIZE: u32 = 24;
const SUPERSAMPLE: u32 = 4;

impl FromWorld for HudShapeAssets {
    fn from_world(world: &mut World) -> Self {
        let triangle = world.resource_mut::<Assets<Image>>().add(triangle_mask());
        Self { triangle }
    }
}

// Apex-up triangle (apex top-center, base across the bottom), supersampled
// so the slanted edges stay smooth at HUD icon sizes.
fn triangle_mask() -> Image {
    let size = TRIANGLE_MASK_SIZE;
    let samples = size * SUPERSAMPLE;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0u32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let fx = (x * SUPERSAMPLE + sx) as f32 + 0.5;
                    let fy = (y * SUPERSAMPLE + sy) as f32 + 0.5;
                    let half_width = fy / samples as f32 * (samples as f32 / 2.0);
                    if (fx - samples as f32 / 2.0).abs() <= half_width {
                        hits += 1;
                    }
                }
            }
            let alpha = (hits * 255 / (SUPERSAMPLE * SUPERSAMPLE)) as u8;
            data.extend([255, 255, 255, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}
