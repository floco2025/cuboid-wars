use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use super::{ProjectileMotion, calculate_projectile_spawns};
use crate::config::MultiShotConfig;
use crate::test_geometry::{BARRIER_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};

// Test copies of the default `projectiles` config values.
const TEST_PROJECTILE_LIFETIME: f32 = 8.0;
const TEST_PROJECTILE_RADIUS: f32 = 0.11;
use crate::physics::CollisionWorld;
use crate::protocol::CarrierId;
use crate::protocol::{Barrier, BarrierKindId, BarrierKindTable, Floor, MapLayout, Position, Ramp, Wall};

fn test_projectile_motion(velocity: Vec3) -> ProjectileMotion {
    ProjectileMotion {
        velocity,
        lifetime: Timer::from_seconds(TEST_PROJECTILE_LIFETIME, TimerMode::Once),
        left_shooter: false,
        radius: TEST_PROJECTILE_RADIUS,
        drag_factor: 0.01,
        bounce_retention: 0.9,
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
        y: f32::from(level) * LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
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
        carrier: CarrierId::WORLD,
    }
}

fn collision_world(walls: &[Wall], floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: walls.to_vec(),
            floors: floors.to_vec(),
            ramps: ramps.to_vec(),
            ..Default::default()
        },
        &crate::protocol::BarrierKindTable::default(),
    )
}

#[test]
fn lower_level_projectile_ignores_upper_level_wall() {
    let pos = Position {
        x: 0.0,
        y: TEST_PROJECTILE_RADIUS,
        z: 0.0,
    };
    let mut lower_motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));
    let mut upper_motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

    assert!(
        lower_motion
            .bounce_at_world_surface(&pos, 0.1, &collision_world(&[test_wall(0)], &[], &[]), &[])
            .is_some()
    );
    assert!(
        upper_motion
            .bounce_at_world_surface(&pos, 0.1, &collision_world(&[test_wall(1)], &[], &[]), &[])
            .is_none()
    );
}

#[test]
fn upper_level_projectile_hits_upper_level_wall() {
    let pos = Position {
        x: 0.0,
        y: LEVEL_HEIGHT + TEST_PROJECTILE_RADIUS,
        z: 0.0,
    };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

    assert!(
        motion
            .bounce_at_world_surface(&pos, 0.1, &collision_world(&[test_wall(1)], &[], &[]), &[])
            .is_some()
    );
}

#[test]
fn world_bounce_reports_first_contact_normal() {
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));
    let bounce = motion
        .bounce_at_world_surface(&pos, 0.1, &collision_world(&[test_wall(0)], &[], &[]), &[])
        .expect("projectile should bounce");

    assert!(bounce.normal.dot(Vec3::NEG_Z) > 0.99);
    assert!(bounce.contact.z < 1.0);
}

#[test]
fn barrier_impact_reports_kind_and_surface_normal() {
    let kind = BarrierKindId(0);
    let table = BarrierKindTable::from_ids(vec!["test".to_owned()]).expect("barrier kind table should build");
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            barriers: vec![Barrier {
                x1: -2.0,
                z1: 1.0,
                x2: 2.0,
                z2: 1.0,
                level: 0,
                levels: 1,
                kind,
                y: 0.0,
                height: WALL_HEIGHT,
                width: BARRIER_THICKNESS,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        &table,
    );
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
    let motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));
    let impact = motion
        .terminate_at_barrier(&pos, 0.1, &world, &[])
        .expect("projectile should hit barrier");

    assert_eq!(impact.kind, kind);
    assert!(impact.normal.dot(Vec3::NEG_Z) > 0.99);
    assert!(impact.point.z < 1.0);
}

#[test]
fn projectile_hits_level_zero_floor_underside() {
    let pos = Position {
        x: 0.0,
        y: -FLOOR_THICKNESS - TEST_PROJECTILE_RADIUS - 0.1,
        z: 0.0,
    };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 10.0, 0.0));

    assert!(
        motion
            .bounce_at_world_surface(&pos, 0.1, &collision_world(&[], &[test_floor(0)], &[]), &[])
            .is_some()
    );
    assert!(motion.velocity.y < 0.0);
}

mod spawning {
    use super::super::spawning::projectile_spawn_is_blocked;
    use crate::physics::CollisionWorld;
    use crate::protocol::CarrierId;
    use crate::protocol::{Floor, MapLayout, Position, Ramp, Wall};
    use crate::test_geometry::{FLOOR_THICKNESS, WALL_THICKNESS};
    use crate::test_geometry::{LEVEL_HEIGHT, WALL_HEIGHT};

    fn test_wall(level: u8) -> Wall {
        Wall {
            x1: -2.0,
            z1: 1.0,
            x2: 2.0,
            z2: 1.0,
            width: WALL_THICKNESS,
            level,
            y: f32::from(level) * LEVEL_HEIGHT,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }
    }

    fn test_floor(level: u8) -> Floor {
        let y = f32::from(level) * LEVEL_HEIGHT;
        Floor {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y,
            thickness: FLOOR_THICKNESS,
            level,
            carrier: CarrierId::WORLD,
        }
    }

    fn test_ramp() -> Ramp {
        Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 8.0,
            carrier: CarrierId::WORLD,
        }
    }

    fn collision_world(walls: &[Wall], ramps: &[Ramp], floors: &[Floor]) -> CollisionWorld {
        CollisionWorld::from_map_layout(
            &MapLayout {
                walls: walls.to_vec(),
                ramps: ramps.to_vec(),
                floors: floors.to_vec(),
                ..Default::default()
            },
            &crate::protocol::BarrierKindTable::default(),
        )
    }

    fn player_eye_height() -> f32 {
        crate::config::gameplay::load_test_gameplay()
            .expect("default gameplay config should load")
            .player
            .eye_height()
    }

    #[test]
    fn spawn_path_ignores_wall_on_different_level() {
        let start = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 2.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[test_wall(1)], &[], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_wall_on_same_upper_level() {
        let y = LEVEL_HEIGHT + player_eye_height();
        let start = Position { x: 0.0, y, z: 0.0 };
        let end = Position { x: 0.0, y, z: 2.0 };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[test_wall(1)], &[], &[]),
            &[]
        ));
        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_when_starting_inside_wall() {
        let start = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 1.0,
        };
        let end = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 2.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_floor_check_catches_crossing_segment() {
        let floor = test_floor(1);
        let start = Position {
            x: 0.0,
            y: LEVEL_HEIGHT + 1.0,
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: LEVEL_HEIGHT - 1.0,
            z: 0.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[], &[], &[floor]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_floor_check_blocks_start_inside() {
        let floor = test_floor(1);
        let start = Position {
            x: 0.0,
            y: LEVEL_HEIGHT,
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: LEVEL_HEIGHT + 1.0,
            z: 0.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[], &[], &[floor]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_allows_ramp_side_escape() {
        let ramp = test_ramp();
        let start = Position { x: 0.2, y: 1.4, z: 4.0 };
        let end = Position {
            x: 0.05,
            y: 1.4,
            z: 4.0,
        };

        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_into_ramp_side() {
        let ramp = test_ramp();
        let start = Position { x: 0.2, y: 1.4, z: 4.0 };
        let end = Position { x: 0.8, y: 1.4, z: 4.0 };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_entering_ramp_from_outside() {
        let ramp = test_ramp();
        let start = Position {
            x: -0.2,
            y: 1.4,
            z: 4.0,
        };
        let end = Position { x: 0.2, y: 1.4, z: 4.0 };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            0.11,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
    }
}

#[test]
fn multi_shot_fires_the_configured_stencil() {
    let mut gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config failed to load");
    gameplay.projectiles.multi_shot =
        MultiShotConfig::from_stencil("multi_shot", 1.5, 1.5, &["x.x", ".o.", "x.x"].map(str::to_owned))
            .expect("stencil rejected");
    let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    let shooter = Position { x: 0.0, y: 1.0, z: 0.0 };
    let (yaw, pitch) = (0.3, 0.1);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;

    let single = calculate_projectile_spawns(&shooter, yaw, pitch, None, 1.6, &gameplay, &world, &[]);
    assert_eq!(single.len(), 1);
    assert!(close(single[0].direction_yaw, yaw) && close(single[0].direction_pitch, pitch));

    let spread = 1.5_f32.to_radians();
    let multi = calculate_projectile_spawns(&shooter, yaw, pitch, Some("test"), 1.6, &gameplay, &world, &[]);
    let offsets: Vec<(f32, f32)> = multi
        .iter()
        .map(|spawn| (spawn.direction_yaw - yaw, spawn.direction_pitch - pitch))
        .collect();
    // Row-major over the stencil; screen-right is negative yaw.
    let expected = [
        (spread, spread),
        (-spread, spread),
        (0.0, 0.0),
        (spread, -spread),
        (-spread, -spread),
    ];
    assert_eq!(offsets.len(), expected.len(), "{offsets:?}");
    for ((yaw_offset, pitch_offset), (want_yaw, want_pitch)) in offsets.iter().zip(expected) {
        assert!(
            close(*yaw_offset, want_yaw) && close(*pitch_offset, want_pitch),
            "{offsets:?}"
        );
    }
}
