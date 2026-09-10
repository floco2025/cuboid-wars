use bevy::math::Vec3;
use bevy::time::{Timer, TimerMode};

use common::{
    config::MultiShotConfig,
    constants::{PORTAL_HALF_WIDTH, TICK_SECS},
    map::Carriers,
    physics::{CollisionWorld, FieldKind, PortalSet},
    protocol::{
        Barrier, BarrierKindId, BarrierKindTable, BridgeKindId, Carrier, CarrierId, Floor, LightBridge, MapLayout,
        Portal, PortalEnd, PortalPairId, Position, Ramp, Wall,
    },
};

use crate::{
    projectiles::{
        MuzzleCheck, ProjectileEvent, ProjectileMotion, calculate_projectile_spawns, earliest_projectile_event,
    },
    test_fixtures::{self, BARRIER_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};

// One allowed pattern, built the way the config loader builds them.
fn multi_shot(column_degrees: f32, row_degrees: f32, stencil: &[&str]) -> MultiShotConfig {
    serde_json::from_value(serde_json::json!({
        "spread_degrees": 1.0,
        "allowed_patterns": ["multi_shot"],
        "patterns": {
            "multi_shot": { "column_scale": column_degrees, "row_scale": row_degrees, "stencil": stencil }
        },
    }))
    .expect("stencil rejected")
}

// Test copies of the default `projectiles` config values.
const TEST_PROJECTILE_LIFETIME: f32 = 8.0;
const TEST_PROJECTILE_RADIUS: f32 = 0.11;

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
        &common::protocol::BarrierKindTable::default(),
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
        .terminate_at_field(&pos, 0.1, &world, &[])
        .expect("projectile should hit barrier");

    assert_eq!(impact.kind, FieldKind::Barrier(kind));
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
    use common::{
        physics::CollisionWorld,
        protocol::{CarrierId, Floor, MapLayout, Position, Ramp, Wall},
    };

    use crate::{
        projectiles::spawning::projectile_spawn_is_blocked,
        test_fixtures::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
    };

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
            &common::protocol::BarrierKindTable::default(),
        )
    }

    fn player_eye_height() -> f32 {
        crate::test_fixtures::gameplay_config().player.eye_height()
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
    let mut gameplay = crate::test_fixtures::gameplay_config();
    gameplay.projectiles.multi_shot = multi_shot(1.5, 1.5, &["x.x", ".o.", "x.x"]);
    let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    let shooter = Position { x: 0.0, y: 1.0, z: 0.0 };
    let (yaw, pitch) = (0.3, 0.1);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;

    let single = calculate_projectile_spawns(&shooter, yaw, pitch, 0, &gameplay, &world, &[], MuzzleCheck::Enforced);
    assert_eq!(single.len(), 1);
    assert!(close(single[0].direction_yaw, yaw) && close(single[0].direction_pitch, pitch));

    let spread = 1.5_f32.to_radians();
    let multi = calculate_projectile_spawns(&shooter, yaw, pitch, 1, &gameplay, &world, &[], MuzzleCheck::Enforced);
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

#[test]
fn a_relayed_volley_reproduces_the_shooters_spawn_set_through_a_blocking_muzzle() {
    let mut gameplay = crate::test_fixtures::gameplay_config();
    gameplay.projectiles.multi_shot = multi_shot(30.0, 30.0, &["xox"]);
    let shooter = Position { x: 0.0, y: 1.0, z: 0.0 };
    let open = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    // A wall beside the shooter that only one muzzle of the volley clips.
    let blocked = collision_world(
        &[Wall {
            x1: 0.5,
            z1: -2.0,
            x2: 0.5,
            z2: 2.0,
            width: WALL_THICKNESS,
            y: 0.0,
            height: WALL_HEIGHT,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        &[],
        &[],
    );
    let spawns = |world: &CollisionWorld, check| {
        calculate_projectile_spawns(&shooter, 0.0, 0.0, 1, &gameplay, world, &[], check)
            .iter()
            .map(|spawn| spawn.direction_yaw)
            .collect::<Vec<_>>()
    };
    let shooter_set = spawns(&open, MuzzleCheck::Enforced);
    assert_eq!(shooter_set.len(), 3);
    assert_eq!(spawns(&blocked, MuzzleCheck::Enforced).len(), 2);
    assert_eq!(spawns(&blocked, MuzzleCheck::Skipped), shooter_set);
}

#[test]
fn powered_bridges_absorb_projectiles_from_both_sides_instead_of_bouncing() {
    let kind = BridgeKindId(0);
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: 2.0,
            thickness: 0.1,
            level: 1,
            kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    for powered in [false, true, false] {
        let powered_kinds = [kind];
        world.set_powered_bridges(if powered { &powered_kinds } else { &[] });
        for (y, velocity) in [(4.0, Vec3::NEG_Y * 40.0), (0.0, Vec3::Y * 40.0)] {
            let pos = Position { x: 0.0, y, z: 0.0 };
            let mut motion = test_projectile_motion(velocity);
            assert!(motion.bounce_at_world_surface(&pos, 0.1, &world, &[]).is_none());
            assert_eq!(motion.field_collision_t(&pos, 0.1, &world, &[]).is_some(), powered);
            let impact = motion.terminate_at_field(&pos, 0.1, &world, &[]);
            assert_eq!(impact.is_some(), powered);
            if let Some(impact) = impact {
                assert_eq!(impact.kind, FieldKind::Bridge(kind));
                assert!(impact.normal.dot(velocity) < 0.0);
            }
            assert_eq!(motion.velocity, velocity);
        }
    }
}

fn portal(end: PortalEnd, pos: Vec3, normal: Vec3) -> Portal {
    Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    }
}

// A portal pair on two one-cell carriers sliding by the given travel per
// tick, posed at tick 1, plus extra world walls.
fn moving_projectile_portals(entry_travel: Vec3, exit_travel: Vec3, obstacles: &[Wall]) -> (CollisionWorld, PortalSet) {
    let carrier = Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Position::default(),
        to: entry_travel.into(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
    };
    let wall = Wall {
        x1: -2.0,
        x2: 2.0,
        z1: -0.1,
        z2: -0.1,
        y: 0.0,
        height: 3.0,
        width: 0.2,
        level: 0,
        carrier: CarrierId(1),
    };
    let layout = MapLayout {
        carriers: vec![
            carrier,
            Carrier {
                from: (Vec3::X * 10.0).into(),
                to: (Vec3::X * 10.0 + exit_travel).into(),
                ..carrier
            },
        ],
        walls: [
            wall,
            Wall {
                carrier: CarrierId(2),
                ..wall
            },
        ]
        .into_iter()
        .chain(obstacles.iter().copied())
        .collect(),
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(0);
    carriers.advance(1);
    world.set_carrier_poses(&carriers);
    let set = PortalSet::rebuild(
        &[
            Portal {
                carrier: CarrierId(1),
                ..portal(PortalEnd::A, Vec3::Y, Vec3::Z)
            },
            Portal {
                carrier: CarrierId(2),
                ..portal(PortalEnd::B, Vec3::Y, Vec3::Z)
            },
        ],
        &world,
        &carriers,
    );
    (world, set)
}

fn projectile_obstacle(z: f32, width: f32) -> Wall {
    Wall {
        x1: -2.0,
        x2: 2.0,
        z1: z,
        z2: z,
        y: 0.0,
        height: 3.0,
        width,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn portal_test_projectile(velocity: Vec3) -> ProjectileMotion {
    let mut config = test_fixtures::gameplay_config().projectiles;
    config.radius = 0.08;
    config.bounce_retention = 1.0;
    ProjectileMotion::from_velocity(velocity, &config)
}

#[test]
fn an_approaching_portals_backing_wall_does_not_win_a_premature_bounce() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let start = Vec3::new(0.0, 1.0, 1.0);
    let velocity = Vec3::NEG_Z * 90.0;
    let hop = set
        .projectile_hop(start, velocity, TICK_SECS, 0.08, TICK_SECS)
        .expect("projectile missed the portal");
    let surface = world.cast_bouncing_ball_excluding(start, velocity * TICK_SECS, 0.08, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), Some(hop.t)),
        ProjectileEvent::Portal
    );

    let outside = Vec3::new(PORTAL_HALF_WIDTH * 2.0, 1.0, 1.0);
    assert!(
        set.projectile_hop(outside, velocity, TICK_SECS, 0.08, TICK_SECS)
            .is_none()
    );
    let surface = world.cast_moving_ball(outside, velocity * TICK_SECS, 0.08);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), None),
        ProjectileEvent::Surface
    );
}

#[test]
fn a_valid_portal_crossing_still_bounces_off_an_unrelated_obstacle() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.18, 0.02)]);
    let start = Position { x: 0.0, y: 1.0, z: 1.0 };
    let mut projectile = portal_test_projectile(Vec3::NEG_Z * 90.0);
    let hop = set
        .projectile_hop(start.into(), projectile.velocity, TICK_SECS, 0.08, TICK_SECS)
        .expect("projectile missed the portal");
    let surface_t = projectile.surface_collision_t(&start, TICK_SECS, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Surface
    );

    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, hop.entry_backing)
        .expect("obstacle did not bounce the projectile");
    assert!((bounce.position.z - 0.27).abs() < 1e-4, "{bounce:?}");
    assert!(projectile.velocity.z > 0.0);
}

#[test]
fn a_bounced_projectile_crosses_a_moving_portal_during_the_same_tick() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.83, 0.2)]);
    let start = Position { x: 0.0, y: 1.0, z: 0.4 };
    let mut projectile = portal_test_projectile(Vec3::Z * 30.0);
    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, &[])
        .expect("projectile missed the bounce wall");
    assert!((bounce.remaining_delta / TICK_SECS - 0.75).abs() < 1e-4);

    let hop = set
        .projectile_hop(
            bounce.position.into(),
            projectile.velocity,
            bounce.remaining_delta,
            0.08,
            TICK_SECS,
        )
        .expect("bounced projectile missed the portal");
    let crossing_tick = 1.0 - bounce.remaining_delta / TICK_SECS * (1.0 - hop.t);
    assert!((hop.entry_point.z - 0.2 * crossing_tick - 0.08).abs() < 1e-4, "{hop:?}");
    let surface_t = projectile.surface_collision_t(&bounce.position, bounce.remaining_delta, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Portal
    );
}
