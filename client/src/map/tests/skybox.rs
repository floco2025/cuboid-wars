use super::*;

fn config() -> LightingConfig {
    LightingConfig {
        bright: SunLighting {
            sky_brightness: 1000.0,
            sun_illuminance: 8000.0,
            ambient_brightness: 70.0,
            sun_disc_luminance: 100.0,
            saturation: 1.0,
        },
        dim: MoonLighting {
            sky_brightness: 64.0,
            moon_illuminance: 200.0,
            ambient_brightness: 30.0,
            moon_disc_luminance: 5.0,
            moon_phase_percent: 60.0,
            saturation: 0.5,
        },
        dark: MoonLighting {
            sky_brightness: 12.0,
            moon_illuminance: 50.0,
            ambient_brightness: 10.0,
            moon_disc_luminance: 0.1,
            moon_phase_percent: 35.0,
            saturation: 0.3,
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
    assert_targets_eq(
        &blend_targets(&config, &wire("bright", "bright", 0.0)),
        &LevelTargets::sun(&config.bright),
    );
    assert_targets_eq(
        &blend_targets(&config, &wire("dim", "dim", 0.0)),
        &LevelTargets::moon(&config.dim),
    );
    assert_targets_eq(
        &blend_targets(&config, &wire("dark", "dark", 0.0)),
        &LevelTargets::moon(&config.dark),
    );
}

#[test]
fn blends_are_halfway_per_channel() {
    let config = config();
    let mid = blend_targets(&config, &wire("bright", "dark", 0.5));
    assert_targets_eq(
        &mid,
        &LevelTargets::sun(&config.bright).interpolate_stable(&LevelTargets::moon(&config.dark), 0.5),
    );
    assert!(
        (mid.phase_percent - LevelTargets::moon(&config.dim).phase_percent).abs() > 1.0,
        "a direct bright↔dark blend must not be the dim look"
    );
}

#[test]
fn blend_factor_clamps() {
    let config = config();
    assert_targets_eq(
        &blend_targets(&config, &wire("bright", "dark", 2.0)),
        &LevelTargets::moon(&config.dark),
    );
    assert_targets_eq(
        &blend_targets(&config, &wire("bright", "dark", -1.0)),
        &LevelTargets::sun(&config.bright),
    );
}

#[test]
fn unknown_preset_falls_back_to_dim() {
    let config = config();
    assert_targets_eq(
        &blend_targets(&config, &wire("sunset", "sunset", 0.0)),
        &LevelTargets::moon(&config.dim),
    );
}

#[test]
fn intensity_channels_blend_in_log_space() {
    let config = config();
    let mid = blend_targets(&config, &wire("bright", "dark", 0.5));
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
