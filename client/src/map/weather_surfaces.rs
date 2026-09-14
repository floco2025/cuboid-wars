use bevy::prelude::*;

use super::grass::GrassMaterials;
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
    materials: Res<GrassMaterials>,
    mut wetness: Local<f32>,
    mut applied: Local<Option<f32>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
) {
    let target = rain.precipitation();
    let rate = if target > *wetness {
        1.0 / WETNESS_SOAK_SECS
    } else {
        1.0 / WETNESS_DRY_SECS
    };
    wetness.smooth_nudge(&target, rate, time.delta_secs());
    const UPDATE_EPSILON: f32 = 0.0005;
    if (target == 0.0 || target == 1.0) && (*wetness - target).abs() < UPDATE_EPSILON {
        *wetness = target;
    }
    if !materials.is_changed()
        && applied.is_some_and(|previous| {
            (*wetness - previous).abs() < UPDATE_EPSILON
                && !((*wetness == 0.0 || *wetness == 1.0) && *wetness != previous)
        })
    {
        return;
    }
    let (Some(mut terrain), Some(mut grass)) = (
        terrain_materials.get_mut(&materials.terrain),
        grass_materials.get_mut(&materials.grass),
    ) else {
        return;
    };
    terrain.extension.weather = Vec4::new(*wetness, WETNESS_DARKENING, WETNESS_ROUGHNESS, 0.0);
    let darkening = 1.0 - (1.0 - WETNESS_DARKENING) * *wetness;
    grass.base.base_color = Color::linear_rgb(darkening, darkening, darkening);
    grass.base.perceptual_roughness = 0.95 + (WETNESS_ROUGHNESS - 0.95) * *wetness;
    *applied = Some(*wetness);
}

#[cfg(test)]
#[path = "tests/weather_surfaces.rs"]
mod tests;
