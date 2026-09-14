use bevy::prelude::*;

#[derive(Clone, Copy)]
pub(in crate::map) struct TerrainCover {
    pub soil: f32,
    pub dry: f32,
    pub shade: f32,
    pub grass_macro: f32,
}

impl TerrainCover {
    pub(in crate::map) fn at(position: Vec2) -> Self {
        // Several incommensurate scales make a stable world-space field with
        // no map-sized repeat. The shader contains the same integer hash,
        // interpolation, and warp, so soil color and blade placement agree.
        let warp = Vec2::new(
            noise(position / 9.0 + Vec2::new(3.0, 71.0)),
            noise(position / 9.0 + Vec2::new(57.0, 13.0)),
        ) * 6.0
            - Vec2::splat(3.0);
        let warped = position + warp;
        let patch = noise(warped / 13.0) * 0.55
            + noise(warped / 5.1 + Vec2::splat(31.0)) * 0.30
            + noise(warped / 2.1 + Vec2::new(13.0, 47.0)) * 0.15;
        Self {
            soil: smooth(0.68, 0.80, patch),
            dry: smooth(0.28, 0.82, noise(position / 37.0 + Vec2::new(71.0, 19.0))),
            shade: {
                let patch = noise(position / 4.8 + Vec2::new(5.0, 29.0)) * 0.65
                    + noise(position / 2.3 + Vec2::new(149.0, 11.0)) * 0.35;
                let region = noise(position / 19.0 + Vec2::new(61.0, 173.0));
                (0.9 + smooth(0.28, 0.72, patch) * 0.2) * (0.95 + region * 0.1)
            },
            grass_macro: noise(position / 4.6 + Vec2::new(211.0, 43.0)) * 0.7
                + noise(position / 2.2 + Vec2::new(17.0, 191.0)) * 0.3,
        }
    }

    // Geometry density follows the same field as the surface: fully bare in
    // brown patches, varied rather than uniform through green areas.
    pub(in crate::map) fn grass_density(self, position: Vec2) -> f32 {
        if self.soil >= 0.28 {
            return 0.0;
        }
        let green = 1.0 - smooth(0.04, 0.28, self.soil);
        let variation = 0.32 + 0.68 * noise(position / 5.3 + Vec2::new(109.0, 7.0));
        green * variation
    }
}

fn smooth(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn noise(position: Vec2) -> f32 {
    let cell = position.floor().as_ivec2();
    let t = position - position.floor();
    let t = t * t * (Vec2::splat(3.0) - t * 2.0);
    let hash = |offset: IVec2| {
        let p = cell + offset;
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
#[path = "tests/cover.rs"]
mod tests;
