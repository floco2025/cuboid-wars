use bevy::prelude::*;

use super::grass::GrassChunks;
use crate::{
    constants::{WETNESS_DARKENING, WETNESS_DRY_SECS, WETNESS_ROUGHNESS, WETNESS_SOAK_SECS},
    materials::{GrassMaterial, TerrainMaterial},
    vfx::WeatherIntensity,
};

// Ground and blades soak while it rains and dry out slowly afterwards. The
// materials are written only when the value moves, since every write
// re-uploads a uniform.
pub fn weather_surfaces_system(
    time: Res<Time>,
    rain: Res<WeatherIntensity>,
    chunks: Res<GrassChunks>,
    mut wetness: Local<f32>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
) {
    let target = rain.precipitation();
    let rate = if target > *wetness {
        1.0 / WETNESS_SOAK_SECS
    } else {
        1.0 / WETNESS_DRY_SECS
    };
    let previous = *wetness;
    wetness.smooth_nudge(&target, rate, time.delta_secs());
    if (*wetness - previous).abs() < 0.0005 {
        return;
    }
    if let Some(mut material) = chunks
        .terrain_material_handle()
        .and_then(|handle| terrain_materials.get_mut(&handle))
    {
        material.extension.weather = Vec4::new(*wetness, WETNESS_DARKENING, WETNESS_ROUGHNESS, 0.0);
    }
    if let Some(mut material) = chunks
        .grass_material_handle()
        .and_then(|handle| grass_materials.get_mut(&handle))
    {
        let darkening = 1.0 - (1.0 - WETNESS_DARKENING) * *wetness;
        material.base.base_color = Color::linear_rgb(darkening, darkening, darkening);
        material.base.perceptual_roughness = 0.95 + (WETNESS_ROUGHNESS - 0.95) * *wetness;
    }
}
