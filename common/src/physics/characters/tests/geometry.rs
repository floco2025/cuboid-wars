use super::*;
use crate::config::gameplay::load_test_gameplay;

#[test]
fn hitbox_and_movement_shapes_are_independent() {
    let mut physics = load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics();
    let movement = character_movement_shape(physics);
    physics.hitbox.width = 2.0;
    let after = character_movement_shape(physics);
    assert_eq!(after.radius, movement.radius);
    assert_eq!(after.segment.a, movement.segment.a);
    assert_eq!(after.segment.b, movement.segment.b);
    let hitbox = character_hitbox_shape(physics);
    physics.movement_collider.diameter = 0.8;
    assert_eq!(character_hitbox_shape(physics), hitbox);
}

#[test]
fn unequal_capsules_collide_during_small_horizontal_steps() {
    let mut small = load_test_gameplay().expect("config").player.physics();
    small.movement_collider.diameter = 0.6;
    small.movement_collider.height = 0.9;
    let mut tall = small;
    tall.movement_collider.diameter = 0.8;
    tall.movement_collider.height = 1.8;
    assert!(character_paths_intersect(
        &Position {
            x: -0.34000087,
            y: -1.2353121e-7,
            z: 9.2637315e-8
        },
        &Position {
            x: -0.28400075,
            y: -4.3437467e-8,
            z: 9.686521e-8
        },
        small,
        &Position {
            x: 0.400001,
            y: -7.92522e-6,
            z: -3.1314897e-8
        },
        &Position {
            x: 0.400001,
            y: -6.7766814e-7,
            z: -3.1314897e-8
        },
        tall,
    ));
}

#[test]
fn capsule_sweeps_use_relative_motion_and_rounded_vertical_clearance() {
    let mut body = load_test_gameplay().expect("config").player.physics();
    body.movement_collider.diameter = 0.6;
    body.movement_collider.height = 0.9;
    let origin = Position::default();
    for (start, end, other_start, other_end, intersects) in [
        (Vec3::NEG_X * 2.0, Vec3::X * 2.0, Vec3::ZERO, Vec3::ZERO, true),
        (Vec3::NEG_X * 2.0, Vec3::X * 2.0, Vec3::Z, Vec3::Z, false),
        (Vec3::ZERO, Vec3::X * 10.0, Vec3::X, Vec3::X * 11.0, false),
        (Vec3::ZERO, Vec3::X * 10.0, Vec3::X, Vec3::X * 9.0, true),
        (Vec3::Y * 2.0, Vec3::NEG_Y * 2.0, Vec3::ZERO, Vec3::ZERO, true),
        (
            Vec3::new(-2.0, 1.0, 0.0),
            Vec3::new(2.0, 1.0, 0.0),
            Vec3::ZERO,
            Vec3::ZERO,
            false,
        ),
        (
            Vec3::new(-2.0, 0.8, 0.0),
            Vec3::new(2.0, 0.8, 0.0),
            Vec3::ZERO,
            Vec3::ZERO,
            true,
        ),
        (
            Vec3::new(-2.0, 0.8, 0.4),
            Vec3::new(2.0, 0.8, 0.4),
            Vec3::ZERO,
            Vec3::ZERO,
            false,
        ),
    ] {
        assert_eq!(
            character_paths_intersect(
                &start.into(),
                &end.into(),
                body,
                &other_start.into(),
                &other_end.into(),
                body
            ),
            intersects,
            "{start:?} -> {end:?} against {other_start:?} -> {other_end:?}"
        );
    }
    assert!(character_paths_intersect(
        &origin, &origin, body, &origin, &origin, body
    ));
}
