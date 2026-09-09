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
mod tests {
    use super::*;

    fn rendering() -> RenderingConfig {
        RenderingConfig {
            opaque_renderer: OpaqueRenderer::Auto,
            directional_shadows: true,
            shadow_map_size: 2048,
            mipmaps: true,
            texture_anisotropy: 8,
            bloom: BloomConfig {
                enabled: true,
                intensity: 0.15,
                threshold: 2.5,
                threshold_softness: 0.4,
            },
        }
    }

    fn error_for(config: RenderingConfig) -> String {
        config
            .validate()
            .expect_err("invalid rendering config accepted")
            .to_string()
    }

    #[test]
    fn zero_texture_anisotropy_is_rejected_by_path() {
        let mut config = rendering();
        config.texture_anisotropy = 0;
        assert!(error_for(config).contains("rendering.texture_anisotropy"));
    }

    #[test]
    fn zero_shadow_map_size_is_rejected_by_path() {
        let mut config = rendering();
        config.shadow_map_size = 0;
        assert!(error_for(config).contains("rendering.shadow_map_size"));
    }

    #[test]
    fn negative_bloom_intensity_is_rejected_by_path() {
        let mut config = rendering();
        config.bloom.intensity = -0.1;
        assert!(error_for(config).contains("rendering.bloom.intensity"));
    }

    #[test]
    fn non_finite_bloom_threshold_is_rejected_by_path() {
        let mut config = rendering();
        config.bloom.threshold = f32::NAN;
        assert!(error_for(config).contains("rendering.bloom.threshold"));
    }

    #[test]
    fn negative_bloom_threshold_softness_is_rejected_by_path() {
        let mut config = rendering();
        config.bloom.threshold_softness = -1.0;
        assert!(error_for(config).contains("rendering.bloom.threshold_softness"));
    }
}
