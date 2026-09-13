use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::constants::TERRAIN_COVER_SIZE;

#[derive(Clone, Copy)]
pub(super) struct TerrainCover {
    pub soil: f32,
    pub dry: f32,
    pub shade: f32,
}

impl TerrainCover {
    pub(super) fn at(position: Vec2) -> Self {
        let patch = noise(position / 16.0, 32) * 0.65 + noise(position / 4.0 + Vec2::splat(31.0), 128) * 0.35;
        Self {
            soil: smooth(0.53, 0.8, patch),
            dry: smooth(0.3, 0.8, noise(position / 32.0 + Vec2::new(71.0, 19.0), 16)),
            shade: 0.78 + noise(position / 8.0 + Vec2::new(5.0, 29.0), 64) * 0.4,
        }
    }

    pub(super) fn image() -> Image {
        let mut data = Vec::with_capacity((TERRAIN_COVER_SIZE * TERRAIN_COVER_SIZE * 4) as usize);
        for z in 0..TERRAIN_COVER_SIZE {
            for x in 0..TERRAIN_COVER_SIZE {
                let cover = Self::at(Vec2::new(x as f32 + 0.5, z as f32 + 0.5));
                data.extend([cover.soil, cover.dry, cover.shade * 0.5, 1.0].map(|v| (v * 255.0).round() as u8));
            }
        }
        let mut image = Image::new(
            Extent3d {
                width: TERRAIN_COVER_SIZE,
                height: TERRAIN_COVER_SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
        image
    }
}

fn smooth(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn noise(position: Vec2, period: i32) -> f32 {
    let cell = position.floor().as_ivec2();
    let t = position - position.floor();
    let t = t * t * (Vec2::splat(3.0) - t * 2.0);
    let hash = |offset: IVec2| {
        let p = (cell + offset).rem_euclid(IVec2::splat(period));
        let mut value = (p.x as u32)
            .wrapping_mul(374761393)
            .wrapping_add((p.y as u32).wrapping_mul(668265263));
        value = (value ^ (value >> 13)).wrapping_mul(1274126177);
        (value ^ (value >> 16)) as f32 / u32::MAX as f32
    };
    hash(IVec2::ZERO)
        .lerp(hash(IVec2::X), t.x)
        .lerp(hash(IVec2::Y).lerp(hash(IVec2::ONE), t.x), t.y)
}

#[cfg(test)]
#[path = "tests/terrain_surface.rs"]
mod tests;
