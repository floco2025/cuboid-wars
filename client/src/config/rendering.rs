use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::validate_non_negative_finite;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpaqueRenderer {
    Auto,
    Forward,
    Deferred,
}

impl OpaqueRenderer {
    #[must_use]
    pub const fn is_deferred(self) -> bool {
        matches!(self, Self::Deferred)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RenderingConfig {
    pub opaque_renderer: OpaqueRenderer,
    pub directional_shadows: bool,
    // Directional shadow map resolution per cascade (Bevy default 2048).
    // Higher halves shadow-edge texel size — matters once the sun moves.
    pub shadow_map_size: u32,
    pub mipmaps: bool,
    pub texture_anisotropy: u16,
    pub bloom: BloomConfig,
}

// Thresholded additive bloom on the main camera (enabling it switches the
// camera to HDR rendering). Pixels below `threshold` are untouched — only
// true HDR emitters (sun disc, projectiles, sparks) overglow.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BloomConfig {
    pub enabled: bool,
    pub intensity: f32,
    // In post-exposure scene units where ~1.0 is white.
    pub threshold: f32,
    // How gradually near-threshold pixels start glowing; raise if glow
    // pops on/off on objects hovering around the threshold.
    pub threshold_softness: f32,
}

impl RenderingConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if self.texture_anisotropy == 0 {
            bail!("rendering.texture_anisotropy must be >= 1");
        }
        if self.shadow_map_size == 0 {
            bail!("rendering.shadow_map_size must be > 0");
        }
        validate_non_negative_finite(self.bloom.intensity, "rendering.bloom.intensity")?;
        validate_non_negative_finite(self.bloom.threshold, "rendering.bloom.threshold")?;
        validate_non_negative_finite(self.bloom.threshold_softness, "rendering.bloom.threshold_softness")?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/rendering.rs"]
mod tests;
