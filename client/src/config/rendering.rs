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
    pub directional_shadows: bool,
    // Directional shadow map resolution per cascade (Bevy default 2048).
    // Higher halves shadow-edge texel size — matters once the sun moves.
    #[serde(default = "default_shadow_map_size")]
    pub shadow_map_size: u32,
    pub mipmaps: bool,
    pub texture_anisotropy: u16,
    #[serde(default)]
    pub bloom: BloomConfig,
}

const fn default_shadow_map_size() -> u32 {
    2048
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
