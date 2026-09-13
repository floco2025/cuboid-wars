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
