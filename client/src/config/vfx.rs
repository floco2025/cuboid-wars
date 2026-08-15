use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct VfxConfig {
    pub max_transient_particles: usize,
    pub projectiles: ProjectileVfxConfig,
    pub actor_beam_in: ActorBeamInVfxConfig,
    pub laser: LaserVfxConfig,
    pub explosions: ExplosionVfxConfig,
}

impl Default for VfxConfig {
    fn default() -> Self {
        Self {
            max_transient_particles: 1_600,
            projectiles: ProjectileVfxConfig::default(),
            actor_beam_in: ActorBeamInVfxConfig::default(),
            laser: LaserVfxConfig::default(),
            explosions: ExplosionVfxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct LaserVfxConfig {
    pub beam_radius: f32,
    pub emissive_brightness: f32,
}

impl Default for LaserVfxConfig {
    fn default() -> Self {
        Self {
            beam_radius: 0.03,
            emissive_brightness: 40.0,
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
    pub full_opacity_duration_secs: f32,
}

impl Default for ExplosionScorchesVfxConfig {
    fn default() -> Self {
        Self {
            blast_diameter_factor: 0.4,
            full_opacity_duration_secs: 30.0,
        }
    }
}

impl VfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if self.max_transient_particles == 0 {
            bail!("vfx.max_transient_particles must be > 0");
        }
        self.projectiles.validate()?;
        self.actor_beam_in.validate()?;
        self.laser.validate()?;
        self.explosions.validate()?;
        if self.projectiles.impact_sparks.base_particle_count > self.max_transient_particles {
            bail!("vfx.projectiles.impact_sparks.base_particle_count must not exceed vfx.max_transient_particles");
        }
        Ok(())
    }
}

impl ProjectileVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.body_emissive_brightness,
            "vfx.projectiles.body_emissive_brightness",
        )?;
        self.impact_sparks.validate()
    }
}

impl ImpactSparksVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
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
    pub(super) fn validate(&self) -> Result<()> {
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

impl LaserVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.beam_radius, "vfx.laser.beam_radius")?;
        validate_non_negative_finite(self.emissive_brightness, "vfx.laser.emissive_brightness")
    }
}

impl ExplosionVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
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
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(
            self.blast_diameter_factor,
            "vfx.explosions.fireball.blast_diameter_factor",
        )?;
        validate_non_negative_finite(self.emissive_brightness, "vfx.explosions.fireball.emissive_brightness")?;
        Ok(())
    }
}

impl ExplosionShockwaveVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.explosions.shockwave.emissive_brightness")
    }
}

impl ExplosionLightVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.intensity_lumens, "vfx.explosions.light.intensity_lumens")
    }
}

impl ExplosionShardsVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
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
    pub(super) fn validate(&self) -> Result<()> {
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
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(
            self.blast_diameter_factor,
            "vfx.explosions.scorches.blast_diameter_factor",
        )?;
        validate_positive_finite(
            self.full_opacity_duration_secs,
            "vfx.explosions.scorches.full_opacity_duration_secs",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vfx_config_is_valid() {
        VfxConfig::default()
            .validate()
            .expect("default VFX config should validate");
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
    fn vfx_config_rejects_zero_laser_beam_radius() {
        let mut config = VfxConfig::default();
        config.laser.beam_radius = 0.0;
        let error = config.validate().expect_err("zero beam radius should fail");
        assert!(error.to_string().contains("beam_radius"));
    }

    #[test]
    fn vfx_config_rejects_explosion_smoke_opacity_above_one() {
        let mut config = VfxConfig::default();
        config.explosions.smoke.max_opacity = 1.1;
        let error = config.validate().expect_err("out-of-range smoke opacity should fail");
        assert!(error.to_string().contains("max_opacity"));
    }
}
