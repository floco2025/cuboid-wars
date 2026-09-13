use super::*;

#[test]
fn initial_fullscreen_selects_primary_monitor() {
    let plugin = window_plugin(WINDOW_SIZE_DEFAULT, true, true, true);
    let window = plugin.primary_window.expect("primary window should be configured");
    assert_eq!(window.mode, WindowMode::BorderlessFullscreen(MonitorSelection::Primary));
}

#[test]
fn hidden_start_is_carried_into_the_window() {
    let plugin = window_plugin(WINDOW_SIZE_DEFAULT, false, true, false);
    let window = plugin.primary_window.expect("primary window should be configured");
    assert!(!window.visible);
}

#[test]
fn windowed_override_wins_over_saved_fullscreen_mode() {
    assert!(!initial_fullscreen(true, Some(true)));
    assert!(initial_fullscreen(false, Some(true)));
    assert!(!initial_fullscreen(false, Some(false)));
}

#[test]
fn cardinal_bearing_and_pitch_convert_to_camera_angles() {
    let north = InitialViewDirection {
        bearing_degrees: 0.0,
        pitch_degrees: 15.0,
    }
    .camera_angles();
    assert!((north.x - std::f32::consts::PI).abs() < 1e-6);
    assert!((north.y - 15.0_f32.to_radians()).abs() < 1e-6);

    let east = InitialViewDirection {
        bearing_degrees: 90.0,
        pitch_degrees: 90.0,
    }
    .camera_angles();
    assert!((east.x - 1.5 * std::f32::consts::PI).abs() < 1e-6);
    assert_eq!(east.y, CAMERA_MAX_PITCH);
}

#[test]
fn settings_overrides_disable_persistence_for_the_review_process() {
    let options = ClientAppOptions {
        force_windowed: true,
        window_x: None,
        window_y: None,
        window_width: Some(1280),
        window_height: Some(720),
        volume: None,
        initial_view: None,
        logging: false,
    };
    assert!(!options.persist_local_settings());

    let ordinary = ClientAppOptions {
        force_windowed: false,
        window_width: None,
        window_height: None,
        ..options
    };
    assert!(ordinary.persist_local_settings());
}
