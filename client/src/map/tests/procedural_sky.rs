use super::*;

#[test]
fn sky_is_continuous_at_the_horizon_and_clear_rain_and_night_differ() {
    for direction in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let (clear, rain, night) = sky_colors(direction);
        assert_eq!(clear, horizon(0.0, 1.0));
        assert_eq!(rain, horizon(1.0, 1.0));
        assert!(night.max_element() < clear.min_element());
        let above = sky_colors((direction + Vec3::Y * 0.00001).normalize()).0;
        assert!(above.distance(clear) < 0.01);
    }
    assert_ne!(sky_colors(Vec3::Y).0, sky_colors(Vec3::Y).1);
}
