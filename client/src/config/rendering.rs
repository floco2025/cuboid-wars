use anyhow::{Result, bail};
use serde::Deserialize;

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
    #[serde(default)]
    pub exclusive_fullscreen: ExclusiveFullscreenConfig,
    pub shadows_directional_enabled: bool,
    // Directional shadow map resolution per cascade (Bevy default 2048).
    // Higher halves shadow-edge texel size — matters once the sun moves.
    #[serde(default = "default_shadow_map_size")]
    pub shadow_map_size: u32,
    pub texture_mipmaps_enabled: bool,
    pub texture_anisotropy: u16,
    pub msaa_samples: u32,
    // Off = present frames immediately (`AutoNoVsync`): a frame that misses
    // the vblank budget shows at e.g. ~58 FPS instead of snapping to 30
    // (Fifo quantization), at the cost of possible tearing.
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    #[serde(default)]
    pub bloom: BloomConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExclusiveFullscreenConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for ExclusiveFullscreenConfig {
    fn default() -> Self {
        Self {
            width: 2560,
            height: 1440,
        }
    }
}

const fn default_shadow_map_size() -> u32 {
    2048
}

const fn default_vsync() -> bool {
    true
}

// Thresholded additive bloom on the main camera (enabling it switches the
// camera to HDR rendering). Pixels below `threshold` are untouched — only
// true HDR emitters (sun disc, projectiles, sparks) overglow.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct BloomConfig {
    pub enabled: bool,
    pub intensity: f32,
    // In post-exposure scene units where ~1.0 is white.
    pub threshold: f32,
    // How gradually near-threshold pixels start glowing; raise if glow
    // pops on/off on objects hovering around the threshold.
    pub threshold_softness: f32,
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 0.15,
            threshold: 1.5,
            threshold_softness: 0.4,
        }
    }
}

impl RenderingConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if !matches!(self.msaa_samples, 1 | 2 | 4 | 8) {
            bail!("rendering.msaa_samples must be one of 1, 2, 4, or 8");
        }
        if self.exclusive_fullscreen.width == 0 || self.exclusive_fullscreen.height == 0 {
            bail!("rendering.exclusive_fullscreen width and height must be > 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ClientSettings;

    #[test]
    fn rendering_config_rejects_zero_fullscreen_dimension() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.rendering.exclusive_fullscreen.width = 0;
        let error = settings
            .rendering
            .validate()
            .expect_err("zero fullscreen width should fail");
        assert!(error.to_string().contains("exclusive_fullscreen"));
    }
}
