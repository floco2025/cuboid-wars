use super::*;

#[test]
fn loudness_is_full_inside_knee_and_falls_off_beyond() {
    let listener = Vec3::ZERO;
    let scale: f32 = 0.1;
    let full_volume_radius = scale.recip();
    assert_eq!(loudness_at_listener(Vec3::ZERO, listener, scale), 1.0);
    assert_eq!(
        loudness_at_listener(Vec3::X * (full_volume_radius * 0.5), listener, scale),
        1.0
    );
    let near = loudness_at_listener(Vec3::X * (full_volume_radius * 2.0), listener, scale);
    let far = loudness_at_listener(Vec3::X * (full_volume_radius * 4.0), listener, scale);
    assert!(near < 1.0);
    assert!(far < near);
}
