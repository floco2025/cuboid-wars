use super::*;

#[test]
fn angle_delta_wraps_at_pi() {
    assert!((angle_delta_radians(3.0, -3.0) - (6.0 - TAU)).abs() < 1e-6);
    assert!((angle_delta_radians(0.1, -0.1) - 0.2).abs() < 1e-6);
}

#[test]
fn sequence_comparison_wraps() {
    assert!(sequence_is_newer(2, 1));
    assert!(!sequence_is_newer(1, 2));
    assert!(!sequence_is_newer(5, 5));
    assert!(sequence_is_newer(0, u32::MAX));
    assert!(!sequence_is_newer(u32::MAX, 0));
}
