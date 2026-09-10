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
