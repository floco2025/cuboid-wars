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
    // Vertical render resolution cap ("1440p"): the 3D scene renders at most
    // this height (width follows the window aspect) and is upscaled to the
    // window; a window smaller than the cap renders native.
    #[serde(default = "default_render_resolution")]
    pub render_resolution: u32,
    pub directional_shadows: bool,
    // Directional shadow map resolution per cascade (Bevy default 2048).
    // Higher halves shadow-edge texel size — matters once the sun moves.
    #[serde(default = "default_shadow_map_size")]
    pub shadow_map_size: u32,
    pub mipmaps: bool,
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

const fn default_render_resolution() -> u32 {
    1440
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
        if self.render_resolution == 0 {
            bail!("rendering.render_resolution must be > 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ClientSettings;

    #[test]
    fn rendering_config_rejects_zero_render_resolution() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.rendering.render_resolution = 0;
        let error = settings
            .rendering
            .validate()
            .expect_err("zero render resolution should fail");
        assert!(error.to_string().contains("render_resolution"));
    }
}
