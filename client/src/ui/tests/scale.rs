use super::*;

#[test]
fn degenerate_width_yields_no_scale() {
    assert_eq!(compute_hud_scale(0.0, 1280.0), None);
    assert_eq!(compute_hud_scale(-100.0, 1280.0), None);
    assert_eq!(compute_hud_scale(f32::NAN, 1280.0), None);
}
