use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use super::ProjectileMotion;
use crate::constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, PROJECTILE_LIFETIME, PROJECTILE_RADIUS, WALL_THICKNESS};
use crate::physics::CollisionWorld;
use crate::protocol::{Floor, MapLayout, Position, Ramp, Wall};

fn test_projectile_motion(velocity: Vec3) -> ProjectileMotion {
    ProjectileMotion {
        velocity,
        lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
    }
}

fn test_wall(level: u8) -> Wall {
    Wall {
        x1: -2.0,
        z1: 1.0,
        x2: 2.0,
        z2: 1.0,
        width: WALL_THICKNESS,
        level,
    }
}

fn test_floor(level: u8) -> Floor {
    Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: f32::from(level) * LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level,
    }
}

fn collision_world(walls: &[Wall], floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        walls: walls.to_vec(),
        floors: floors.to_vec(),
        ramps: ramps.to_vec(),
        ..Default::default()
    })
}

#[test]
fn lower_level_projectile_ignores_upper_level_wall() {
    let pos = Position {
        x: 0.0,
        y: PROJECTILE_RADIUS,
        z: 0.0,
    };
    let mut lower_motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));
    let mut upper_motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

    assert!(
        lower_motion
            .resolve_world_bounces(&pos, 0.1, &collision_world(&[test_wall(0)], &[], &[]))
            .is_some()
    );
    assert!(
        upper_motion
            .resolve_world_bounces(&pos, 0.1, &collision_world(&[test_wall(1)], &[], &[]))
            .is_none()
    );
}

#[test]
fn upper_level_projectile_hits_upper_level_wall() {
    let pos = Position {
        x: 0.0,
        y: LEVEL_HEIGHT + PROJECTILE_RADIUS,
        z: 0.0,
    };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

    assert!(
        motion
            .resolve_world_bounces(&pos, 0.1, &collision_world(&[test_wall(1)], &[], &[]))
            .is_some()
    );
}

#[test]
fn projectile_hits_level_zero_floor_underside() {
    let pos = Position {
        x: 0.0,
        y: -FLOOR_THICKNESS - PROJECTILE_RADIUS - 0.1,
        z: 0.0,
    };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 10.0, 0.0));

    assert!(
        motion
            .resolve_world_bounces(&pos, 0.1, &collision_world(&[], &[test_floor(0)], &[]))
            .is_some()
    );
    assert!(motion.velocity.y < 0.0);
}
