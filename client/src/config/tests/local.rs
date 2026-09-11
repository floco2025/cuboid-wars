use super::*;
use crate::test_fixtures;

fn sample() -> LocalSettings {
    LocalSettings {
        version: LOCAL_SETTINGS_VERSION,
        fullscreen: true,
        window_x: Some(120),
        window_y: Some(80),
        window_width: 1600,
        window_height: 900,
        master_volume: 0.8,
        preferences: UserPreferences {
            fullscreen_resolution: 1080,
            vsync: true,
            msaa_samples: 2,
            portal_view_budget: 2,
            mouse_sensitivity: 1.5,
            zoom_sensitivity: 1.5,
            invert_y: true,
            fov_degrees: 100.0,
            shake_scale: 0.5,
            show_diagnostics: false,
            rearview_mirror: true,
        },
    }
}

#[test]
fn local_settings_round_trip() {
    let path = std::env::temp_dir().join(format!("cuboid_local_round_trip_{}.json", std::process::id()));
    let saved = sample();
    saved.save_to_path(&path).expect("local settings failed to save");
    let loaded = LocalSettings::load_from_path(&path).expect("saved local settings failed to load");
    assert_eq!(saved, loaded);
    let mut settings = test_fixtures::client_settings();
    loaded.apply_to(&mut settings);
    assert_eq!(settings.preferences, saved.preferences);
    std::fs::remove_file(&path).ok();
}

#[test]
fn local_settings_save_replaces_existing_file() {
    let path = std::env::temp_dir().join(format!("cuboid_local_replace_{}.json", std::process::id()));
    std::fs::write(&path, "incomplete").expect("existing settings fixture failed to write");
    let saved = sample();

    saved.save_to_path(&path).expect("local settings failed to replace");

    let loaded = LocalSettings::load_from_path(&path).expect("replaced local settings failed to load");
    assert_eq!(saved, loaded);
    let mut settings = test_fixtures::client_settings();
    loaded.apply_to(&mut settings);
    assert_eq!(settings.preferences, saved.preferences);
    std::fs::remove_file(&path).ok();
}

#[test]
fn stale_version_is_ignored() {
    let path = std::env::temp_dir().join(format!("cuboid_local_stale_{}.json", std::process::id()));
    let mut stale = sample();
    stale.version = LOCAL_SETTINGS_VERSION + 1;
    stale.save_to_path(&path).expect("stale local settings failed to save");
    assert!(LocalSettings::load_from_path(&path).is_none());
    std::fs::remove_file(&path).ok();
}
