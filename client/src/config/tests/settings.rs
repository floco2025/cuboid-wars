use super::*;

#[test]
fn shipped_client_config_loads_and_validates() {
    ClientSettings::load_default().expect("shipped client config should load and validate");
}

#[test]
fn preferences_reject_zero_fullscreen_resolution() {
    let mut settings = ClientSettings::load_default().expect("shipped client config should load");
    settings.preferences.fullscreen_resolution = 0;
    let error = settings
        .preferences
        .validate()
        .expect_err("zero render resolution should fail");
    assert!(error.to_string().contains("fullscreen_resolution"));
}

#[test]
fn preferences_reject_portal_budget_above_settings_maximum() {
    let mut settings = ClientSettings::load_default().expect("shipped client config should load");
    settings.preferences.portal_view_budget = 9;
    let error = settings
        .preferences
        .validate()
        .expect_err("oversized portal view budget should fail");
    assert!(error.to_string().contains("portal_view_budget"));
}
#[test]
fn json_cannot_override_runtime_preference_defaults() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../../../config/client/client.json")).expect("client JSON is invalid");
    json["preferences"] = serde_json::json!({"fov_degrees": 10.0, "zoom_sensitivity": 100.0});
    let settings: ClientSettings = serde_json::from_value(json).expect("client settings are invalid");
    settings.validate().expect("default preferences are invalid");
    assert_eq!(settings.preferences.fov_degrees, CAMERA_FOV_DEGREES_DEFAULT);
    assert_eq!(settings.preferences.zoom_sensitivity, INPUT_ZOOM_SENSITIVITY_DEFAULT);
}
