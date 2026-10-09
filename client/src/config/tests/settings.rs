use super::*;
use crate::test_fixtures;

#[test]
fn invalid_preferences_name_their_field() {
    let defaults = test_fixtures::client_settings();
    let mut zero_resolution = defaults.clone();
    zero_resolution.preferences.fullscreen_resolution = 0;
    let mut oversized_budget = defaults.clone();
    oversized_budget.preferences.portal_view_budget = 9;
    for (settings, field) in [
        (zero_resolution, "fullscreen_resolution"),
        (oversized_budget, "portal_view_budget"),
    ] {
        let error = settings
            .preferences
            .validate()
            .expect_err("invalid preference accepted");
        assert!(error.to_string().contains(field), "unexpected error: {error}");
    }
    for name in ["footstep_volume_db", "actor_movement_volume_db"] {
        for db in [
            -20.0,
            -6.0,
            0.0,
            6.0,
            20.0,
            -21.0,
            21.0,
            f32::NAN,
            f32::NEG_INFINITY,
            f32::INFINITY,
        ] {
            let mut settings = defaults.clone();
            match name {
                "footstep_volume_db" => settings.preferences.footstep_volume_db = db,
                _ => settings.preferences.actor_movement_volume_db = db,
            }
            let result = settings.preferences.validate();
            if (-20.0..=20.0).contains(&db) {
                result.expect("valid sound adjustment rejected");
            } else {
                assert!(
                    result
                        .expect_err("invalid sound adjustment accepted")
                        .to_string()
                        .contains(name)
                );
            }
        }
    }
}

#[test]
fn sky_controls_reject_invalid_ranges_and_allow_zero_luminance() {
    let mut settings = test_fixtures::client_settings();
    settings.sky.sun.luminance = 0.0;
    settings.sky.moon.luminance = 0.0;
    settings.sky.stars.luminance = 0.0;
    settings.validate().expect("zero luminance was rejected");

    for (invalid, field) in [
        (
            {
                let mut value = settings.clone();
                value.sky.sun.size_scale = SKY_MAX_BODY_SIZE_SCALE + 0.1;
                value
            },
            "sun.size_scale",
        ),
        (
            {
                let mut value = settings.clone();
                value.sky.stars.luminance = f32::NAN;
                value
            },
            "stars.luminance",
        ),
        (
            {
                let mut value = settings.clone();
                value.sky.clouds.clear_coverage = 0.8;
                value.sky.clouds.overcast_coverage = 0.4;
                value
            },
            "clouds.clear_coverage",
        ),
        (
            {
                let mut value = settings.clone();
                value.sky.moon.size_scale = 0.0;
                value
            },
            "moon.size_scale",
        ),
        (
            {
                let mut value = settings.clone();
                value.sky.clouds.movement_speed_degrees_per_second = f32::INFINITY;
                value
            },
            "clouds.movement_speed_degrees_per_second",
        ),
    ] {
        let error = invalid.validate().expect_err("invalid sky tuning was accepted");
        assert!(error.to_string().contains(field), "unexpected error: {error}");
    }
}

#[test]
fn lighting_controls_reject_invalid_ranges_and_allow_unquantized_shadows() {
    let mut settings = test_fixtures::client_settings();
    settings.lighting.shadow_step_degrees = 0.0;
    settings.validate().expect("zero shadow step was rejected");

    settings.lighting.shadow_step_degrees = 180.0;
    let error = settings.validate().expect_err("oversized shadow step was accepted");
    assert!(error.to_string().contains("shadow_step_degrees"));

    let mut settings = test_fixtures::client_settings();
    settings.lighting.night_ambient_brightness = -0.1;
    let error = settings.validate().expect_err("negative night ambient was accepted");
    assert!(error.to_string().contains("night_ambient_brightness"));
}
