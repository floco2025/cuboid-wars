use super::*;
use crate::{map::setup_scene_lighting_system, test_fixtures};
use bevy::light::cluster::GlobalClusterSettings;

#[test]
fn selected_sky_controls_light_direction_with_or_without_a_visible_disc() {
    for show_disc in [false, true] {
        let mut settings = test_fixtures::map_settings();
        settings.skybox = "custom".to_owned();
        let mut source: serde_json::Value =
            serde_json::from_str(test_fixtures::ASSETS_JSON).expect("asset fixture invalid");
        source["skyboxes"]["custom"] = source["skyboxes"]["test"].clone();
        source["skyboxes"]["custom"]["celestial_disc"]["direction"] = serde_json::json!([-3.0, 2.0, 1.0]);
        source["skyboxes"]["custom"]["celestial_disc"]["show"] = serde_json::json!(show_disc);
        let assets: AssetSet = serde_json::from_value(source).expect("custom skybox rejected");
        let mut app = App::new();
        app.insert_resource(settings)
            .insert_resource(test_fixtures::client_settings())
            .insert_resource(assets)
            .insert_resource(GlobalClusterSettings {
                supports_storage_buffers: false,
                clustered_decals_are_usable: false,
                gpu_clustering: None,
                max_uniform_buffer_clusterable_objects: 0,
                view_cluster_bindings_max_indices: 0,
            })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, (setup_scene_lighting_system, setup_sky_disc_system));
        app.update();
        let world = app.world_mut();
        let (light, transform) = world
            .query_filtered::<(&DirectionalLight, &Transform), With<CelestialLightMarker>>()
            .single(world)
            .expect("scene light missing");
        assert!(Vec3::from(transform.back()).abs_diff_eq(Vec3::new(-3.0, 2.0, 1.0).normalize(), 1e-5));
        assert!(light.illuminance > 0.0);
        assert_eq!(world.contains_resource::<SkyDiscAssets>(), show_disc);
    }
}

fn config() -> LightingConfig {
    LightingConfig {
        bright: SunLighting {
            sky_brightness: 1000.0,
            sun_illuminance: 8000.0,
            ambient_brightness: 70.0,
            saturation: 1.0,
        },
        dim: MoonLighting {
            sky_brightness: 64.0,
            moon_illuminance: 200.0,
            ambient_brightness: 30.0,
            saturation: 0.5,
        },
        dark: MoonLighting {
            sky_brightness: 12.0,
            moon_illuminance: 50.0,
            ambient_brightness: 10.0,
            saturation: 0.3,
        },
    }
}

fn disc_config() -> CelestialDiscDef {
    CelestialDiscDef {
        direction: [1.0, 2.0, 3.0],
        show: true,
        distance: 500.0,
        radius: 10.0,
        bright: CelestialDiscLook {
            luminance: 100.0,
            phase_percent: 100.0,
        },
        dim: CelestialDiscLook {
            luminance: 5.0,
            phase_percent: 60.0,
        },
        dark: CelestialDiscLook {
            luminance: 0.1,
            phase_percent: 35.0,
        },
    }
}

fn wire(from: &str, to: &str, blend: f32) -> LightingBlend {
    LightingBlend {
        from: from.to_owned(),
        to: to.to_owned(),
        blend,
    }
}

fn assert_targets_eq(actual: &LevelTargets, expected: &LevelTargets) {
    assert!((actual.sky - expected.sky).abs() < 1e-3, "sky");
    assert!((actual.illuminance - expected.illuminance).abs() < 1e-3, "illuminance");
    assert!((actual.ambient - expected.ambient).abs() < 1e-3, "ambient");
    assert!((actual.disc - expected.disc).abs() < 1e-3, "disc");
    assert!(
        (actual.phase_percent - expected.phase_percent).abs() < 1e-3,
        "phase_percent"
    );
    assert!(actual.color.abs_diff_eq(expected.color, 1e-3), "color");
    assert!((actual.saturation - expected.saturation).abs() < 1e-3, "saturation");
}

#[test]
fn presets_resolve_to_their_configured_looks() {
    let config = config();
    let disc = disc_config();
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("bright", "bright", 0.0)),
        &LevelTargets::sun(&config.bright, &disc.bright),
    );
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("dim", "dim", 0.0)),
        &LevelTargets::moon(&config.dim, &disc.dim),
    );
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("dark", "dark", 0.0)),
        &LevelTargets::moon(&config.dark, &disc.dark),
    );
}

#[test]
fn blends_are_halfway_per_channel() {
    let config = config();
    let disc = disc_config();
    let mid = blend_targets(&config, &disc, &wire("bright", "dark", 0.5));
    assert_targets_eq(
        &mid,
        &LevelTargets::sun(&config.bright, &disc.bright)
            .interpolate_stable(&LevelTargets::moon(&config.dark, &disc.dark), 0.5),
    );
    assert!(
        (mid.phase_percent - LevelTargets::moon(&config.dim, &disc.dim).phase_percent).abs() > 1.0,
        "a direct bright↔dark blend must not be the dim look"
    );
}

#[test]
fn blend_factor_clamps() {
    let config = config();
    let disc = disc_config();
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("bright", "dark", 2.0)),
        &LevelTargets::moon(&config.dark, &disc.dark),
    );
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("bright", "dark", -1.0)),
        &LevelTargets::sun(&config.bright, &disc.bright),
    );
}

#[test]
fn unknown_preset_falls_back_to_dim() {
    let config = config();
    let disc = disc_config();
    assert_targets_eq(
        &blend_targets(&config, &disc, &wire("sunset", "sunset", 0.0)),
        &LevelTargets::moon(&config.dim, &disc.dim),
    );
}

#[test]
fn intensity_channels_blend_in_log_space() {
    let config = config();
    let disc = disc_config();
    let mid = blend_targets(&config, &disc, &wire("bright", "dark", 0.5));
    // A log-domain midpoint is the geometric mean in linear terms —
    // perceptually halfway, unlike the arithmetic mean.
    let expected = (config.bright.sun_illuminance * config.dark.moon_illuminance).sqrt();
    let actual = linear_intensity(mid.illuminance);
    assert!(
        (actual - expected).abs() / expected < 1e-3,
        "expected geometric mean {expected}, got {actual}"
    );
}

#[test]
fn zero_intensity_round_trips_to_off() {
    assert_eq!(linear_intensity(log_intensity(0.0)), 0.0);
    assert!(linear_intensity(log_intensity(5.0)) > 4.9);
}

#[test]
fn sky_disc_child_transform_preserves_world_direction_for_each_camera() {
    let camera = Transform::from_xyz(4.0, 2.0, -3.0).with_rotation(Quat::from_euler(EulerRot::YXZ, 1.2, -0.4, 0.2));
    let away = Vec3::new(0.3, 0.8, -0.5).normalize();
    let local = sky_disc_local_transform(&camera, away, 400.0);
    let world_translation = camera.translation + camera.rotation * local.translation;
    let world_rotation = camera.rotation * local.rotation;

    assert!(world_translation.abs_diff_eq(camera.translation + away * 400.0, 1e-4));
    assert!((world_rotation * Vec3::NEG_Z).abs_diff_eq(away, 1e-5));
}
