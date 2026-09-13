use bevy::{
    asset::embedded_asset,
    material::OpaqueRendererMethod,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
};

use crate::{
    config::SkyConfig,
    constants::{
        SKY_BRIGHT_STAR_FRACTION, SKY_CLOUD_COLOR, SKY_CLOUD_SCALE, SKY_DAY_HORIZON_COLOR, SKY_DAY_ZENITH_COLOR,
        SKY_MOON_APPARENT_RADIUS_DEGREES, SKY_MOON_CRATER_CONTRAST, SKY_MOON_EARTHSHINE, SKY_MOON_HALO_LUMINANCE,
        SKY_MOON_HALO_SIZE_DEGREES, SKY_NIGHT_HORIZON_COLOR, SKY_NIGHT_ZENITH_COLOR, SKY_OVERCAST_COLOR,
        SKY_STAR_LUMINANCE_MAX_FACTOR, SKY_STAR_LUMINANCE_MIN_FACTOR, SKY_STAR_SEED, SKY_STAR_TWINKLE,
        SKY_SUN_APPARENT_RADIUS_DEGREES, SKY_SUN_HALO_LUMINANCE, SKY_SUN_HALO_SIZE_DEGREES, SKY_SUNSET_COLOR,
        SKY_TWILIGHT_HORIZON_COLOR, SKY_TWILIGHT_ZENITH_COLOR,
    },
};

const SKY_SHADER: &str = "embedded://client/materials/sky.wgsl";

// Configured controls and stable visual constants become plain uniforms so
// one shared material can serve every scene camera. Vectors use their fourth
// lane for a related scalar to keep the bind group portable to WebGL's
// alignment rules.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct ProceduralSkyMaterial {
    #[uniform(0)]
    pub sun_direction: Vec4,
    #[uniform(1)]
    pub moon_direction: Vec4,
    #[uniform(2)]
    pub pole_rotation: Vec4,
    #[uniform(3)]
    pub time_weather_phase: Vec4,
    #[uniform(4)]
    pub day_horizon: Vec4,
    #[uniform(5)]
    pub day_zenith: Vec4,
    #[uniform(6)]
    pub sunset: Vec4,
    #[uniform(7)]
    pub twilight_horizon: Vec4,
    #[uniform(8)]
    pub twilight_zenith: Vec4,
    #[uniform(9)]
    pub night_horizon: Vec4,
    #[uniform(10)]
    pub night_zenith: Vec4,
    #[uniform(11)]
    pub sun: Vec4,
    #[uniform(12)]
    pub moon: Vec4,
    #[uniform(13)]
    pub moon_halo: Vec4,
    #[uniform(14)]
    pub stars: Vec4,
    #[uniform(15)]
    pub star_detail: Vec4,
    #[uniform(16)]
    pub clouds: Vec4,
    #[uniform(17)]
    pub cloud_color: Vec4,
    #[uniform(18)]
    pub overcast_color: Vec4,
}

fn color(value: [f32; 3], scalar: f32) -> Vec4 {
    Vec4::new(value[0], value[1], value[2], scalar)
}

impl ProceduralSkyMaterial {
    #[must_use]
    pub fn from_config(config: SkyConfig) -> Self {
        Self {
            sun_direction: Vec4::Y,
            moon_direction: -Vec4::Y,
            pole_rotation: Vec4::Z,
            time_weather_phase: Vec4::ZERO,
            day_horizon: color(SKY_DAY_HORIZON_COLOR, config.day_brightness),
            day_zenith: color(SKY_DAY_ZENITH_COLOR, config.day_brightness),
            sunset: color(SKY_SUNSET_COLOR, 0.0),
            twilight_horizon: color(SKY_TWILIGHT_HORIZON_COLOR, config.twilight_brightness),
            twilight_zenith: color(SKY_TWILIGHT_ZENITH_COLOR, config.twilight_brightness),
            night_horizon: color(SKY_NIGHT_HORIZON_COLOR, config.night_brightness),
            night_zenith: color(SKY_NIGHT_ZENITH_COLOR, config.night_brightness),
            sun: Vec4::new(
                (SKY_SUN_APPARENT_RADIUS_DEGREES * config.sun.size_scale).to_radians(),
                config.sun.luminance,
                SKY_SUN_HALO_SIZE_DEGREES.to_radians(),
                SKY_SUN_HALO_LUMINANCE,
            ),
            moon: Vec4::new(
                (SKY_MOON_APPARENT_RADIUS_DEGREES * config.moon.size_scale).to_radians(),
                config.moon.luminance,
                SKY_MOON_EARTHSHINE,
                SKY_MOON_CRATER_CONTRAST,
            ),
            moon_halo: Vec4::new(
                SKY_MOON_HALO_SIZE_DEGREES.to_radians(),
                SKY_MOON_HALO_LUMINANCE,
                0.0,
                0.0,
            ),
            stars: Vec4::new(
                SKY_STAR_SEED as f32,
                config.stars.density,
                config.stars.luminance * SKY_STAR_LUMINANCE_MIN_FACTOR,
                config.stars.luminance * SKY_STAR_LUMINANCE_MAX_FACTOR,
            ),
            star_detail: Vec4::new(SKY_BRIGHT_STAR_FRACTION, SKY_STAR_TWINKLE, 0.0, 0.0),
            clouds: Vec4::new(
                config.clouds.clear_coverage,
                config.clouds.overcast_coverage,
                SKY_CLOUD_SCALE,
                0.0,
            ),
            cloud_color: color(SKY_CLOUD_COLOR, 0.0),
            overcast_color: color(SKY_OVERCAST_COLOR, 0.0),
        }
    }
}

impl Material for ProceduralSkyMaterial {
    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn opaque_render_method(&self) -> OpaqueRendererMethod {
        // The sky is emissive and has no G-buffer representation. Keeping it
        // in the forward opaque pass works with both world renderer modes.
        OpaqueRendererMethod::Forward
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Cameras live inside the sphere. It never writes depth; normal
        // reverse-Z testing leaves nearer map geometry in front regardless
        // of opaque draw ordering.
        descriptor.primitive.cull_mode = Some(Face::Front);
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

pub struct ProceduralSkyMaterialPlugin;

impl Plugin for ProceduralSkyMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "sky.wgsl");
        app.add_plugins(MaterialPlugin::<ProceduralSkyMaterial>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        constants::{SKY_MOON_APPARENT_RADIUS_DEGREES, SKY_SUN_APPARENT_RADIUS_DEGREES},
        test_fixtures,
    };

    #[test]
    fn body_size_scales_multiply_real_apparent_radii() {
        let config = test_fixtures::client_settings().sky;
        let material = ProceduralSkyMaterial::from_config(config);
        assert!((material.sun.x - (SKY_SUN_APPARENT_RADIUS_DEGREES * config.sun.size_scale).to_radians()).abs() < 1e-6);
        assert!(
            (material.moon.x - (SKY_MOON_APPARENT_RADIUS_DEGREES * config.moon.size_scale).to_radians()).abs() < 1e-6
        );
        assert!(
            config.sun.size_scale > 1.0,
            "shipped sun should be creatively exaggerated"
        );
        assert!(
            config.moon.size_scale > 1.0,
            "shipped moon should be creatively exaggerated"
        );
    }
}
