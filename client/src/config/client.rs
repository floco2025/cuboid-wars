use std::{fs, path::Path};

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

const SUPPORTED_VERSION: u32 = 1;

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

// Three-way map debug-color mode. Cycled at runtime via the C key.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DebugColorMode {
    // Real materials (textures from `assets.json`).
    #[default]
    Off,
    // One color per material name (deterministic hash → HSV).
    ByMaterial,
    // One color per record sent in `MapLayout` (random per batch).
    BySegment,
}

impl DebugColorMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::ByMaterial,
            Self::ByMaterial => Self::BySegment,
            Self::BySegment => Self::Off,
        }
    }
}

// Top-level client config. Loaded from `config/client/client.json` once at
// startup; fields are immutable after that (no hot-reload).
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ClientSettings {
    pub version: u32,
    pub rendering: RenderingConfig,
    pub lighting: LightingConfig,
    pub camera: CameraConfig,
    pub input: InputConfig,
    pub hud: HudConfig,
    pub barriers: BarriersConfig,
    #[serde(default)]
    pub grass: GrassConfig,
    #[serde(default)]
    pub vfx: VfxConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    pub debug: DebugConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub spatial_distance_scale: f32,
    pub explosion_gain_multiplier: f32,
    pub projectile_impacts: ProjectileImpactAudioConfig,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            spatial_distance_scale: 0.1,
            explosion_gain_multiplier: 2.0,
            projectile_impacts: ProjectileImpactAudioConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ProjectileImpactAudioConfig {
    pub wall_bounce_gain_multiplier: f32,
    pub min_bounce_speed_meters_per_second: f32,
    pub max_bounce_sounds_per_second: f32,
    pub rate_limit_preemption_loudness_ratio: f32,
}

impl Default for ProjectileImpactAudioConfig {
    fn default() -> Self {
        Self {
            wall_bounce_gain_multiplier: 0.2,
            min_bounce_speed_meters_per_second: 10.0,
            max_bounce_sounds_per_second: 30.0,
            rate_limit_preemption_loudness_ratio: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct VfxConfig {
    pub max_transient_particles: usize,
    pub projectiles: ProjectileVfxConfig,
    pub actor_beam_in: ActorBeamInVfxConfig,
    pub explosions: ExplosionVfxConfig,
}

impl Default for VfxConfig {
    fn default() -> Self {
        Self {
            max_transient_particles: 1_600,
            projectiles: ProjectileVfxConfig::default(),
            actor_beam_in: ActorBeamInVfxConfig::default(),
            explosions: ExplosionVfxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ProjectileVfxConfig {
    pub body_emissive_brightness: f32,
    pub impact_sparks: ImpactSparksVfxConfig,
}

impl Default for ProjectileVfxConfig {
    fn default() -> Self {
        Self {
            body_emissive_brightness: 10.0,
            impact_sparks: ImpactSparksVfxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ImpactSparksVfxConfig {
    pub base_particle_count: usize,
    pub particle_size: f32,
    pub particle_speed: f32,
    pub particle_lifetime_secs: f32,
    pub emissive_brightness: f32,
}

impl Default for ImpactSparksVfxConfig {
    fn default() -> Self {
        Self {
            base_particle_count: 6,
            particle_size: 0.05,
            particle_speed: 8.0,
            particle_lifetime_secs: 0.25,
            emissive_brightness: 25.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ActorBeamInVfxConfig {
    pub sparkles_per_m3_per_second: f32,
    pub sparkle_size: f32,
    pub sparkle_lifetime_secs: f32,
    pub sparkle_emissive_brightness: f32,
    pub light_intensity_lumens_per_m3: f32,
    pub materialization_ring_enabled: bool,
}

impl Default for ActorBeamInVfxConfig {
    fn default() -> Self {
        Self {
            sparkles_per_m3_per_second: 200.0,
            sparkle_size: 0.05,
            sparkle_lifetime_secs: 1.5,
            sparkle_emissive_brightness: 25.0,
            light_intensity_lumens_per_m3: 500_000.0,
            materialization_ring_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionVfxConfig {
    pub base_duration_secs: f32,
    pub fireball: ExplosionFireballVfxConfig,
    pub shockwave: ExplosionShockwaveVfxConfig,
    pub light: ExplosionLightVfxConfig,
    pub shards: ExplosionShardsVfxConfig,
    pub smoke: ExplosionSmokeVfxConfig,
    pub scorches: ExplosionScorchesVfxConfig,
}

impl Default for ExplosionVfxConfig {
    fn default() -> Self {
        Self {
            base_duration_secs: 0.8,
            fireball: ExplosionFireballVfxConfig::default(),
            shockwave: ExplosionShockwaveVfxConfig::default(),
            light: ExplosionLightVfxConfig::default(),
            shards: ExplosionShardsVfxConfig::default(),
            smoke: ExplosionSmokeVfxConfig::default(),
            scorches: ExplosionScorchesVfxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionFireballVfxConfig {
    pub blast_diameter_factor: f32,
    pub emissive_brightness: f32,
}

impl Default for ExplosionFireballVfxConfig {
    fn default() -> Self {
        Self {
            blast_diameter_factor: 0.5,
            emissive_brightness: 3_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionShockwaveVfxConfig {
    pub emissive_brightness: f32,
}

impl Default for ExplosionShockwaveVfxConfig {
    fn default() -> Self {
        Self {
            emissive_brightness: 2_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionLightVfxConfig {
    pub intensity_lumens: f32,
}

impl Default for ExplosionLightVfxConfig {
    fn default() -> Self {
        Self {
            intensity_lumens: 1_000_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionShardsVfxConfig {
    pub count_per_radius_meter: f32,
    pub size: f32,
    pub emissive_brightness: f32,
}

impl Default for ExplosionShardsVfxConfig {
    fn default() -> Self {
        Self {
            count_per_radius_meter: 40.0,
            size: 0.20,
            emissive_brightness: 2_500.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionSmokeVfxConfig {
    pub count_per_radius_meter: f32,
    pub end_size: f32,
    pub lifetime_secs: f32,
    pub max_opacity: f32,
}

impl Default for ExplosionSmokeVfxConfig {
    fn default() -> Self {
        Self {
            count_per_radius_meter: 1.5,
            end_size: 1.45,
            lifetime_secs: 4.0,
            max_opacity: 0.32,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ExplosionScorchesVfxConfig {
    pub blast_diameter_factor: f32,
    pub lifetime_secs: f32,
}

impl Default for ExplosionScorchesVfxConfig {
    fn default() -> Self {
        Self {
            blast_diameter_factor: 0.4,
            lifetime_secs: 30.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RenderingConfig {
    pub opaque_renderer: OpaqueRenderer,
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

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LightingConfig {
    pub ambient_brightness: f32,
    pub directional_brightness: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CameraConfig {
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    // Padding factor applied around the visible map when fitting the
    // top-down camera. 1.0 = exact fit, >1.0 = room around edges.
    pub topdown_margin: f32,
    pub topdown_tilt_degrees: f32,
    pub rearview: RearviewConfig,
    #[serde(default)]
    pub shake: CameraShakeConfig,
}

// Directional camera shake on the local player: projectile hits shake along
// the incoming shot direction (with a small vertical companion), hard
// landings shake vertically. Both share one duration/intensity envelope.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct CameraShakeConfig {
    pub duration_secs: f32,
    pub intensity: f32,
    pub hit_vertical: f32,
    pub fall_vertical: f32,
    pub blast_vertical: f32,
}

impl Default for CameraShakeConfig {
    fn default() -> Self {
        Self {
            duration_secs: 0.3,
            intensity: 3.0,
            hit_vertical: 0.2,
            fall_vertical: 0.5,
            blast_vertical: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RearviewConfig {
    pub enabled: bool,
    pub fov_degrees: f32,
    // Width / height of the rearview viewport as a fraction of the window
    // dimensions. The inset from the window edge is a fixed `HUD_EDGE_MARGIN_PX`
    // shared with the HUD panels, not a ratio.
    pub width_ratio: f32,
    pub height_ratio: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InputConfig {
    // Mouse look sensitivity in radians per pixel. Larger = faster turn
    // per inch of mouse movement.
    pub mouse_sensitivity: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HudConfig {
    // Window width (logical px) the configured HUD sizes are designed for.
    // The whole screen-space HUD scales by window_width / reference_width.
    #[serde(default = "default_hud_reference_width")]
    pub reference_width: f32,
    pub font_sizes: FontSizesConfig,
    pub message_feed: MessageFeedConfig,
    pub floating_labels: FloatingLabelsConfig,
    pub health_bars: HealthBarsConfig,
    pub banner: BannerConfig,
    pub quest_panel: QuestPanelConfig,
    pub death_overlay: DeathOverlayConfig,
}

const fn default_hud_reference_width() -> f32 {
    1280.0
}

// Per-purpose font sizes. Each surface has its own preferred size; the
// floating-label one is much larger because it has to fill a small texture
// before being scaled down onto a world-space quad.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FontSizesConfig {
    // Player-list names and the RTT / FPS readouts.
    pub player_list: f32,
    // Score column in the player list. Often larger than `player_list`
    // since it's the headline number.
    pub score: f32,
    // Bottom-right game-message feed lines.
    pub message_feed: f32,
    // Name text inside the 3D floating label above a character.
    pub floating_label: f32,
    // Centered HUD banner ("Collect 10 Gold!", "You died!", etc.).
    pub banner: f32,
    // Quest-panel rows (top-right): title + progress counter.
    pub quest_panel: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MessageFeedConfig {
    pub entry_duration_secs: f32,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FloatingLabelsConfig {
    pub height_above_character: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HealthBarsConfig {
    // Height ÷ width of the world-space bar above a character. The width is
    // fixed (`LABEL_PLAYER_BAR_WIDTH` / `LABEL_ACTOR_MESH_WIDTH`, meters);
    // only the proportions are tunable.
    pub floating_player_aspect: f32,
    pub floating_actor_aspect: f32,
    // On-screen bar under each player-list row, in logical px.
    pub player_list_width: f32,
    pub player_list_height: f32,
}

// Per-quest progress bar in the top-right quest panel (colors are consts in
// `constants.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct QuestPanelConfig {
    pub bar_width: f32,
    pub bar_height: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BannerConfig {
    pub quest_announcement_duration_secs: f32,
    pub quest_completed_duration_secs: f32,
    pub death_duration_secs: f32,
    pub death_text: String,
    pub fade_duration_secs: f32,
}

// Red full-screen death tint. Timer-driven (no fade in): snaps on at
// peak alpha when the player dies, holds, and fades out over the final
// `fade_duration_secs` before disappearing at `duration_secs`. Peak
// alpha is currently a const (`DEATH_OVERLAY_MAX_ALPHA` in
// `players/death.rs`); expose here if it ever needs tuning.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DeathOverlayConfig {
    pub duration_secs: f32,
    pub fade_duration_secs: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BarriersConfig {
    // Pulse alpha swings between `alpha_min` and `alpha_max` at the
    // configured rate. Below ~0.1 the barrier almost disappears (good
    // off-phase look); above ~0.7 it reads as solid.
    pub alpha_min: f32,
    pub alpha_max: f32,
    pub pulse_hz: f32,
}

// Performance/feel knobs for the decorative grass. Pure-appearance numbers
// (blade shape, colors) are module constants in `map/spawn/grass.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct GrassConfig {
    pub enabled: bool,
    pub tufts_per_m2: f32,
    // Horizontal sway amplitude at the blade tip, in meters.
    pub wind_strength: f32,
    // Sway oscillation speed, in radians per second.
    pub wind_speed: f32,
    pub wind_direction_degrees: f32,
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tufts_per_m2: 12.0,
            wind_strength: 0.05,
            wind_speed: 1.5,
            wind_direction_degrees: 30.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DebugConfig {
    pub collider_boxes: bool,
}

impl ClientSettings {
    pub fn load_default() -> Result<Self> {
        let settings = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/client/client.json"
        )))?;
        settings.validate()?;
        Ok(settings)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        if self.version != SUPPORTED_VERSION {
            bail!(
                "unsupported client config version {} (expected {})",
                self.version,
                SUPPORTED_VERSION
            );
        }
        self.rendering.validate()?;
        self.lighting.validate()?;
        self.camera.validate()?;
        self.input.validate()?;
        self.hud.validate()?;
        self.barriers.validate()?;
        self.grass.validate()?;
        self.vfx.validate()?;
        self.audio.validate()?;
        Ok(())
    }
}

impl AudioConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.spatial_distance_scale, "audio.spatial_distance_scale")?;
        validate_non_negative_finite(self.explosion_gain_multiplier, "audio.explosion_gain_multiplier")?;
        self.projectile_impacts.validate()
    }
}

impl ProjectileImpactAudioConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.wall_bounce_gain_multiplier,
            "audio.projectile_impacts.wall_bounce_gain_multiplier",
        )?;
        validate_non_negative_finite(
            self.min_bounce_speed_meters_per_second,
            "audio.projectile_impacts.min_bounce_speed_meters_per_second",
        )?;
        validate_positive_finite(
            self.max_bounce_sounds_per_second,
            "audio.projectile_impacts.max_bounce_sounds_per_second",
        )?;
        if !(self.rate_limit_preemption_loudness_ratio.is_finite() && self.rate_limit_preemption_loudness_ratio >= 1.0)
        {
            bail!("audio.projectile_impacts.rate_limit_preemption_loudness_ratio must be finite and at least 1.0");
        }
        Ok(())
    }
}

impl VfxConfig {
    fn validate(&self) -> Result<()> {
        if self.max_transient_particles == 0 {
            bail!("vfx.max_transient_particles must be > 0");
        }
        self.projectiles.validate()?;
        self.actor_beam_in.validate()?;
        self.explosions.validate()?;
        if self.projectiles.impact_sparks.base_particle_count > self.max_transient_particles {
            bail!("vfx.projectiles.impact_sparks.base_particle_count must not exceed vfx.max_transient_particles");
        }
        Ok(())
    }
}

impl ProjectileVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.body_emissive_brightness,
            "vfx.projectiles.body_emissive_brightness",
        )?;
        self.impact_sparks.validate()
    }
}

impl ImpactSparksVfxConfig {
    fn validate(&self) -> Result<()> {
        if self.base_particle_count == 0 {
            bail!("vfx.projectiles.impact_sparks.base_particle_count must be > 0");
        }
        validate_positive_finite(self.particle_size, "vfx.projectiles.impact_sparks.particle_size")?;
        validate_non_negative_finite(self.particle_speed, "vfx.projectiles.impact_sparks.particle_speed")?;
        validate_positive_finite(
            self.particle_lifetime_secs,
            "vfx.projectiles.impact_sparks.particle_lifetime_secs",
        )?;
        validate_non_negative_finite(
            self.emissive_brightness,
            "vfx.projectiles.impact_sparks.emissive_brightness",
        )?;
        Ok(())
    }
}

impl ActorBeamInVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.sparkles_per_m3_per_second,
            "vfx.actor_beam_in.sparkles_per_m3_per_second",
        )?;
        validate_positive_finite(self.sparkle_size, "vfx.actor_beam_in.sparkle_size")?;
        validate_positive_finite(self.sparkle_lifetime_secs, "vfx.actor_beam_in.sparkle_lifetime_secs")?;
        validate_non_negative_finite(
            self.sparkle_emissive_brightness,
            "vfx.actor_beam_in.sparkle_emissive_brightness",
        )?;
        validate_non_negative_finite(
            self.light_intensity_lumens_per_m3,
            "vfx.actor_beam_in.light_intensity_lumens_per_m3",
        )?;
        Ok(())
    }
}

impl ExplosionVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.base_duration_secs, "vfx.explosions.base_duration_secs")?;
        self.fireball.validate()?;
        self.shockwave.validate()?;
        self.light.validate()?;
        self.shards.validate()?;
        self.smoke.validate()?;
        self.scorches.validate()?;
        Ok(())
    }
}

impl ExplosionFireballVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(
            self.blast_diameter_factor,
            "vfx.explosions.fireball.blast_diameter_factor",
        )?;
        validate_non_negative_finite(self.emissive_brightness, "vfx.explosions.fireball.emissive_brightness")?;
        Ok(())
    }
}

impl ExplosionShockwaveVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.explosions.shockwave.emissive_brightness")
    }
}

impl ExplosionLightVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.intensity_lumens, "vfx.explosions.light.intensity_lumens")
    }
}

impl ExplosionShardsVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.count_per_radius_meter,
            "vfx.explosions.shards.count_per_radius_meter",
        )?;
        validate_positive_finite(self.size, "vfx.explosions.shards.size")?;
        validate_non_negative_finite(self.emissive_brightness, "vfx.explosions.shards.emissive_brightness")?;
        Ok(())
    }
}

impl ExplosionSmokeVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.count_per_radius_meter,
            "vfx.explosions.smoke.count_per_radius_meter",
        )?;
        validate_positive_finite(self.end_size, "vfx.explosions.smoke.end_size")?;
        validate_positive_finite(self.lifetime_secs, "vfx.explosions.smoke.lifetime_secs")?;
        validate_unit_ratio(self.max_opacity, "vfx.explosions.smoke.max_opacity")?;
        Ok(())
    }
}

impl ExplosionScorchesVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(
            self.blast_diameter_factor,
            "vfx.explosions.scorches.blast_diameter_factor",
        )?;
        validate_positive_finite(self.lifetime_secs, "vfx.explosions.scorches.lifetime_secs")?;
        Ok(())
    }
}

impl RenderingConfig {
    fn validate(&self) -> Result<()> {
        if !matches!(self.msaa_samples, 1 | 2 | 4 | 8) {
            bail!("rendering.msaa_samples must be one of 1, 2, 4, or 8");
        }
        Ok(())
    }
}

impl LightingConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.ambient_brightness, "lighting.ambient_brightness")?;
        validate_non_negative_finite(self.directional_brightness, "lighting.directional_brightness")?;
        Ok(())
    }
}

impl CameraConfig {
    fn validate(&self) -> Result<()> {
        validate_fov(self.fov_first_person_degrees, "camera.fov_first_person_degrees")?;
        validate_fov(self.fov_top_down_degrees, "camera.fov_top_down_degrees")?;
        validate_positive_finite(self.topdown_margin, "camera.topdown_margin")?;
        validate_positive_finite(self.topdown_tilt_degrees, "camera.topdown_tilt_degrees")?;
        self.rearview.validate()?;
        Ok(())
    }
}

impl RearviewConfig {
    fn validate(&self) -> Result<()> {
        validate_fov(self.fov_degrees, "camera.rearview.fov_degrees")?;
        validate_unit_ratio(self.width_ratio, "camera.rearview.width_ratio")?;
        validate_unit_ratio(self.height_ratio, "camera.rearview.height_ratio")?;
        Ok(())
    }
}

impl InputConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.mouse_sensitivity, "input.mouse_sensitivity")
    }
}

impl HudConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.reference_width, "hud.reference_width")?;
        validate_positive_finite(self.font_sizes.player_list, "hud.font_sizes.player_list")?;
        validate_positive_finite(self.font_sizes.score, "hud.font_sizes.score")?;
        validate_positive_finite(self.font_sizes.message_feed, "hud.font_sizes.message_feed")?;
        validate_positive_finite(self.font_sizes.floating_label, "hud.font_sizes.floating_label")?;
        validate_positive_finite(self.font_sizes.banner, "hud.font_sizes.banner")?;
        validate_positive_finite(self.font_sizes.quest_panel, "hud.font_sizes.quest_panel")?;
        validate_non_negative_finite(
            self.message_feed.entry_duration_secs,
            "hud.message_feed.entry_duration_secs",
        )?;
        if self.message_feed.max_entries == 0 {
            bail!("hud.message_feed.max_entries must be > 0");
        }
        validate_positive_finite(
            self.floating_labels.height_above_character,
            "hud.floating_labels.height_above_character",
        )?;
        validate_positive_finite(
            self.health_bars.floating_player_aspect,
            "hud.health_bars.floating_player_aspect",
        )?;
        validate_positive_finite(
            self.health_bars.floating_actor_aspect,
            "hud.health_bars.floating_actor_aspect",
        )?;
        validate_positive_finite(self.health_bars.player_list_width, "hud.health_bars.player_list_width")?;
        validate_positive_finite(
            self.health_bars.player_list_height,
            "hud.health_bars.player_list_height",
        )?;
        validate_positive_finite(
            self.banner.quest_announcement_duration_secs,
            "hud.banner.quest_announcement_duration_secs",
        )?;
        validate_positive_finite(
            self.banner.quest_completed_duration_secs,
            "hud.banner.quest_completed_duration_secs",
        )?;
        validate_positive_finite(self.banner.death_duration_secs, "hud.banner.death_duration_secs")?;
        if self.banner.death_text.is_empty() {
            bail!("hud.banner.death_text must not be empty");
        }
        validate_positive_finite(self.banner.fade_duration_secs, "hud.banner.fade_duration_secs")?;
        validate_positive_finite(self.quest_panel.bar_width, "hud.quest_panel.bar_width")?;
        validate_positive_finite(self.quest_panel.bar_height, "hud.quest_panel.bar_height")?;
        validate_positive_finite(self.death_overlay.duration_secs, "hud.death_overlay.duration_secs")?;
        validate_positive_finite(
            self.death_overlay.fade_duration_secs,
            "hud.death_overlay.fade_duration_secs",
        )?;
        Ok(())
    }
}

impl BarriersConfig {
    fn validate(&self) -> Result<()> {
        validate_unit_ratio(self.alpha_min, "barriers.alpha_min")?;
        validate_unit_ratio(self.alpha_max, "barriers.alpha_max")?;
        if self.alpha_min > self.alpha_max {
            bail!("barriers.alpha_min must be <= barriers.alpha_max");
        }
        validate_positive_finite(self.pulse_hz, "barriers.pulse_hz")?;
        Ok(())
    }
}

impl GrassConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.tufts_per_m2, "grass.tufts_per_m2")?;
        validate_non_negative_finite(self.wind_strength, "grass.wind_strength")?;
        validate_non_negative_finite(self.wind_speed, "grass.wind_speed")?;
        if !self.wind_direction_degrees.is_finite() {
            bail!("grass.wind_direction_degrees must be finite");
        }
        Ok(())
    }
}

fn validate_positive_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value > 0.0) {
        bail!("{name} must be positive and finite");
    }
    Ok(())
}

fn validate_non_negative_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value >= 0.0) {
        bail!("{name} must be finite and non-negative");
    }
    Ok(())
}

fn validate_fov(fov_degrees: f32, name: &str) -> Result<()> {
    if !(1.0..179.0).contains(&fov_degrees) {
        bail!("{name} must be greater than 1 and less than 179");
    }
    Ok(())
}

fn validate_unit_ratio(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
        bail!("{name} must be in [0.0, 1.0]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_client_config_loads_and_validates() {
        ClientSettings::load_default().expect("shipped client config should load and validate");
    }

    #[test]
    fn default_vfx_config_is_valid() {
        VfxConfig::default()
            .validate()
            .expect("default VFX config should validate");
    }

    #[test]
    fn default_audio_config_is_valid() {
        AudioConfig::default()
            .validate()
            .expect("default audio config should validate");
    }

    #[test]
    fn vfx_config_rejects_zero_particle_budget() {
        let config = VfxConfig {
            max_transient_particles: 0,
            ..VfxConfig::default()
        };
        let error = config.validate().expect_err("zero particle budget should fail");
        assert!(error.to_string().contains("max_transient_particles"));
    }

    #[test]
    fn vfx_config_rejects_zero_impact_count() {
        let mut config = VfxConfig::default();
        config.projectiles.impact_sparks.base_particle_count = 0;
        let error = config.validate().expect_err("zero impact count should fail");
        assert!(error.to_string().contains("base_particle_count"));
    }

    #[test]
    fn vfx_config_rejects_explosion_smoke_opacity_above_one() {
        let mut config = VfxConfig::default();
        config.explosions.smoke.max_opacity = 1.1;
        let error = config.validate().expect_err("out-of-range smoke opacity should fail");
        assert!(error.to_string().contains("max_opacity"));
    }

    #[test]
    fn audio_config_rejects_zero_spatial_distance_scale() {
        let config = AudioConfig {
            spatial_distance_scale: 0.0,
            ..AudioConfig::default()
        };
        let error = config.validate().expect_err("zero spatial distance scale should fail");
        assert!(error.to_string().contains("spatial_distance_scale"));
    }

    #[test]
    fn audio_config_rejects_preemption_ratio_below_one() {
        let mut config = AudioConfig::default();
        config.projectile_impacts.rate_limit_preemption_loudness_ratio = 0.9;
        let error = config.validate().expect_err("preemption ratio below one should fail");
        assert!(error.to_string().contains("rate_limit_preemption_loudness_ratio"));
    }
}
