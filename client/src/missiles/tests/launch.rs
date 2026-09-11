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

#[test]
fn a_blocked_runway_is_resampled_and_a_boxed_in_muzzle_falls_back_to_the_aim() {
    use crate::test_fixtures::{WALL_HEIGHT, WALL_THICKNESS};
    use common::protocol::{CarrierId, MapLayout, Wall};

    let wall = |x1, z1, x2, z2| Wall {
        x1,
        z1,
        x2,
        z2,
        width: WALL_THICKNESS,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let world = |walls: Vec<Wall>| {
        CollisionWorld::from_map_layout(&MapLayout {
            walls,
            ..Default::default()
        })
    };
    let muzzle = Vec3::new(0.0, 1.5, 0.0);
    let spread = 60.0_f32.to_radians();
    let mut rng = rand::rng();

    // One wall behind the muzzle: every sample the resample keeps flies clear.
    let open = world(vec![wall(-6.0, -1.0, 6.0, -1.0)]);
    for _ in 0..50 {
        let direction = clear_launch_direction(Vec3::Z, spread, muzzle, 4.0, 0.3, &open, &[], &mut rng);
        assert!(sweep_clear(&open, &[], muzzle, direction * 4.0, 0.3));
    }

    // Boxed in on every side: the aim itself comes back.
    let boxed = world(vec![
        wall(-2.0, -1.0, 2.0, -1.0),
        wall(-2.0, 1.0, 2.0, 1.0),
        wall(-1.0, -2.0, -1.0, 2.0),
        wall(1.0, -2.0, 1.0, 2.0),
    ]);
    assert_eq!(
        clear_launch_direction(Vec3::Z, spread, muzzle, 4.0, 0.3, &boxed, &[], &mut rng),
        Vec3::Z
    );
}
