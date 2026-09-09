use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub color: [f32; 3],
    pub flicker: bool,
    pub scene: String,
    pub scale: f32,
    pub offset_from_wall: f32,
    pub brightness: f32,
    pub range: f32,
    pub radius: f32,
    pub emissive_luminance: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkyboxDef {
    // Path to a cube-cross layout image used to derive the cubemap faces.
    pub image: String,
    pub brightness: f32,
    // Seconds per full ambient sky turn; 0 (or absent) = static sky.
    #[serde(default)]
    pub rotation_period_secs: f32,
    // Sun rotation advances in discrete steps of this size so shadow maps
    // stay pixel-stable between steps; 0 (or absent) = continuous (shadow
    // edges shimmer while the sun creeps).
    #[serde(default)]
    pub sun_step_degrees: f32,
    pub sun_disc: SunDiscDef,
}

// The visible sun: a camera-following emissive sphere along the directional
// light's incoming direction, so it always sits where the shadows say.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SunDiscDef {
    // Far enough that map geometry reads in front of it, inside the 1000 m
    // far plane. `radius: 0` disables the disc.
    pub distance: f32,
    pub radius: f32,
    // Emissive luminance (cd/m²) — must dwarf the skybox `brightness` so the
    // disc tonemaps to clipped white, and drives the bloom glare halo.
    pub luminance: f32,
}
