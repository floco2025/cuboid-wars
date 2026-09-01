use anyhow::{Result, bail};
use serde::Deserialize;

pub const MAX_PORTAL_RECURSION_DEPTH: u8 = 3;

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
    // Initial fullscreen render height ("1440p", adjustable in the settings
    // menu): in fullscreen the 3D scene renders at most this height (width
    // follows the aspect) and is upscaled to the monitor. Windowed mode
    // always renders at the window size.
    #[serde(default = "default_fullscreen_resolution")]
    pub fullscreen_resolution: u32,
    pub directional_shadows: bool,
    // Directional shadow map resolution per cascade (Bevy default 2048).
    // Higher halves shadow-edge texel size — matters once the sun moves.
    #[serde(default = "default_shadow_map_size")]
    pub shadow_map_size: u32,
    pub mipmaps: bool,
    pub texture_anisotropy: u16,
    pub msaa_samples: u32,
    // Number of additional see-through portals rendered inside a portal view.
    pub portal_recursion_depth: u8,
    // Portal views rendered per frame, largest on screen first; the rest show their glow.
    pub portal_view_budget: u8,
    // Off = present frames immediately (`AutoNoVsync`): a frame that misses
    // the vblank budget shows at e.g. ~58 FPS instead of snapping to 30
    // (Fifo quantization), at the cost of possible tearing.
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    #[serde(default)]
    pub bloom: BloomConfig,
}

const fn default_fullscreen_resolution() -> u32 {
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
        if self.fullscreen_resolution == 0 {
            bail!("rendering.fullscreen_resolution must be > 0");
        }
        if self.portal_recursion_depth > MAX_PORTAL_RECURSION_DEPTH {
            bail!("rendering.portal_recursion_depth must be in [0, {MAX_PORTAL_RECURSION_DEPTH}]");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MAX_PORTAL_RECURSION_DEPTH;
    use crate::config::ClientSettings;

    #[test]
    fn rendering_config_rejects_zero_fullscreen_resolution() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.rendering.fullscreen_resolution = 0;
        let error = settings
            .rendering
            .validate()
            .expect_err("zero render resolution should fail");
        assert!(error.to_string().contains("fullscreen_resolution"));
    }

    #[test]
    fn rendering_config_rejects_excessive_portal_recursion() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.rendering.portal_recursion_depth = MAX_PORTAL_RECURSION_DEPTH + 1;
        let error = settings
            .rendering
            .validate()
            .expect_err("excessive portal recursion should fail");
        assert!(error.to_string().contains("portal_recursion_depth"));
    }
}
