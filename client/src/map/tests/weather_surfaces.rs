use super::*;
use crate::{map::grass::fixtures, vfx::rain_smoothing_system};
use std::time::Duration;

fn app() -> App {
    let mut app = fixtures::app();
    app.init_resource::<Time>()
        .init_resource::<WeatherIntensity>()
        .add_systems(
            Update,
            (rain_smoothing_system, weather_surfaces_system)
                .chain()
                .after(crate::map::grass::setup_grass_materials_system),
        );
    app
}

fn step(app: &mut App, seconds: f32, raining: bool) {
    let mut rain = app.world_mut().resource_mut::<WeatherIntensity>();
    rain.raining = raining;
    rain.target = if raining { 1.0 } else { 0.0 };
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(seconds));
    app.update();
}

fn wetness(app: &App) -> f32 {
    let handle = &app.world().resource::<GrassMaterials>().terrain;
    app.world()
        .resource::<Assets<TerrainMaterial>>()
        .get(handle)
        .expect("terrain missing")
        .extension
        .weather
        .x
}

#[test]
fn ground_dries_at_every_frame_rate() {
    for fps in [30, 60, 144] {
        let mut app = app();
        step(&mut app, 60.0, true);
        assert_eq!(wetness(&app), 1.0);
        for _ in 0..fps * 60 {
            step(&mut app, 1.0 / fps as f32, false);
        }
        assert!(
            (0.3..0.45).contains(&wetness(&app)),
            "drying at {fps} FPS: {}",
            wetness(&app)
        );
        for _ in 0..fps * 540 {
            step(&mut app, 1.0 / fps as f32, false);
        }
        assert_eq!(wetness(&app), 0.0);
        let handle = &app.world().resource::<GrassMaterials>().grass;
        let grass = app
            .world()
            .resource::<Assets<GrassMaterial>>()
            .get(handle)
            .expect("grass missing");
        assert_eq!(grass.base.base_color, Color::linear_rgb(1.0, 1.0, 1.0));
        assert_eq!(grass.base.perceptual_roughness, 0.95);
    }
}

#[test]
fn replaced_material_receives_current_wetness_even_when_weather_is_stable() {
    let mut app = app();
    step(&mut app, 60.0, true);
    let previous = app.world().resource::<GrassMaterials>().terrain.clone();
    let mut assets = app.world_mut().resource_mut::<Assets<TerrainMaterial>>();
    let mut replacement = assets.get(&previous).expect("terrain missing").clone();
    replacement.extension.weather = Vec4::ZERO;
    let replacement = assets.add(replacement);
    app.world_mut().resource_mut::<GrassMaterials>().terrain = replacement;
    step(&mut app, 0.0, true);
    assert_eq!(wetness(&app), 1.0);
}
