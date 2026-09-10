use super::*;

#[test]
fn launch_direction_stays_within_spread() {
    let mut rng = rand::rng();
    let aim = Vec3::new(0.3, 0.5, 0.8).normalize();
    let spread = 50.0_f32.to_radians();
    for _ in 0..100 {
        let launched = launch_direction(aim, spread, &mut rng);
        assert!((launched.length() - 1.0).abs() < 1e-4, "direction stays unit length");
        let angle = aim.angle_between(launched);
        assert!(
            (spread / 2.0 - 1e-3..=spread + 1e-3).contains(&angle),
            "deviation {angle} outside [spread/2, spread]"
        );
    }
}

#[test]
fn launch_direction_zero_spread_is_straight() {
    let mut rng = rand::rng();
    let aim = Vec3::Z;
    assert_eq!(launch_direction(aim, 0.0, &mut rng), aim);
}
