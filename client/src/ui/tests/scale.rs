use super::*;

#[test]
fn scale_is_ratio_of_width_to_reference() {
    assert_eq!(compute_hud_scale(1280.0, 1280.0), Some(1.0));
    assert_eq!(compute_hud_scale(1920.0, 1280.0), Some(1.5));
    assert_eq!(compute_hud_scale(640.0, 1280.0), Some(0.5));
}

#[test]
fn tiny_window_clamps_to_min_scale() {
    assert_eq!(compute_hud_scale(320.0, 1280.0), Some(HUD_MIN_SCALE));
}

#[test]
fn degenerate_width_yields_no_scale() {
    assert_eq!(compute_hud_scale(0.0, 1280.0), None);
    assert_eq!(compute_hud_scale(-100.0, 1280.0), None);
    assert_eq!(compute_hud_scale(f32::NAN, 1280.0), None);
}
