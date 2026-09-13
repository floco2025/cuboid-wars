use super::*;
use crate::test_fixtures;

#[test]
fn preferences_reject_zero_fullscreen_resolution() {
    let mut settings = test_fixtures::client_settings();
    settings.preferences.fullscreen_resolution = 0;
    let error = settings
        .preferences
        .validate()
        .expect_err("zero render resolution should fail");
    assert!(error.to_string().contains("fullscreen_resolution"));
}

#[test]
fn preferences_reject_portal_budget_above_settings_maximum() {
    let mut settings = test_fixtures::client_settings();
    settings.preferences.portal_view_budget = 9;
    let error = settings
        .preferences
        .validate()
        .expect_err("oversized portal view budget should fail");
    assert!(error.to_string().contains("portal_view_budget"));
}

#[test]
fn sound_volumes_default_to_neutral_and_validate_the_db_range() {
    let defaults = test_fixtures::client_settings();
    assert_eq!(defaults.preferences.footstep_volume_db, 0.0);
    assert_eq!(defaults.preferences.actor_movement_volume_db, 0.0);
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
fn json_cannot_override_runtime_preference_defaults() {
    let mut json: serde_json::Value =
        serde_json::from_str(test_fixtures::SETTINGS_JSON).expect("client JSON is invalid");
    json["preferences"] = serde_json::json!({"fov_degrees": 10.0, "zoom_sensitivity": 100.0});
    let settings: ClientSettings = serde_json::from_value(json).expect("client settings are invalid");
    settings.validate().expect("default preferences are invalid");
    assert_eq!(settings.preferences.fov_degrees, CAMERA_FOV_DEGREES_DEFAULT);
    assert_eq!(settings.preferences.zoom_sensitivity, INPUT_ZOOM_SENSITIVITY_DEFAULT);
}

#[test]
fn sky_controls_reject_invalid_ranges_and_allow_zero_luminance() {
    let mut settings = test_fixtures::client_settings();
    settings.sky.sun.luminance = 0.0;
    settings.sky.moon.luminance = 0.0;
    settings.sky.stars.luminance = 0.0;
    settings
        .validate()
        .expect("zero luminance should disable a sky emitter");

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
    settings
        .validate()
        .expect("zero shadow step should disable direction quantization");

    settings.lighting.shadow_step_degrees = 180.0;
    let error = settings.validate().expect_err("oversized shadow step was accepted");
    assert!(error.to_string().contains("shadow_step_degrees"));

    let mut settings = test_fixtures::client_settings();
    settings.lighting.night_ambient_brightness = -0.1;
    let error = settings.validate().expect_err("negative night ambient was accepted");
    assert!(error.to_string().contains("night_ambient_brightness"));
}

#[test]
fn removed_low_level_sky_settings_are_rejected() {
    for (path, field) in [
        (&["sky", "sun"][..], "angular_radius_degrees"),
        (&["sky", "moon"][..], "crater_contrast"),
        (&["sky", "stars"][..], "seed"),
        (&["sky", "stars"][..], "twinkle"),
        (&["sky", "clouds"][..], "scale"),
        (&["lighting"][..], "night_saturation"),
    ] {
        let mut json: serde_json::Value =
            serde_json::from_str(test_fixtures::SETTINGS_JSON).expect("client JSON is invalid");
        let mut object = &mut json;
        for segment in path {
            object = &mut object[*segment];
        }
        object[field] = serde_json::json!(1.0);
        let error = serde_json::from_value::<ClientSettings>(json).expect_err("removed sky setting was accepted");
        assert!(error.to_string().contains(field), "unexpected error: {error}");
    }
}

#[test]
fn removed_grass_density_setting_is_rejected() {
    let mut json: serde_json::Value =
        serde_json::from_str(test_fixtures::SETTINGS_JSON).expect("client JSON is invalid");
    json["grass"]["tufts_per_m2"] = serde_json::json!(16.0);
    let error = serde_json::from_value::<ClientSettings>(json).expect_err("removed grass setting was accepted");
    assert!(error.to_string().contains("tufts_per_m2"), "unexpected error: {error}");
}
