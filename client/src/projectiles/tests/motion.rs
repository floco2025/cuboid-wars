use bevy::{
    math::Vec3,
    time::{Timer, TimerMode},
};
use common::{
    constants::TICK_SECS,
    map::Carriers,
    physics::{CollisionWorld, PortalSet},
    protocol::{
        Carrier, CarrierId, FieldId, Floor, LightBridge, MapLayout, Portal, PortalEnd, Position, SwitchState, Wall,
    },
};

use crate::{
    projectiles::{ProjectileEvent, ProjectileMotion, earliest_projectile_event},
    test_fixtures::{self, FLOOR_THICKNESS, PORTAL_HALF_WIDTH, WALL_HEIGHT, WALL_THICKNESS, portal},
};

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

fn test_wall() -> Wall {
    Wall {
        x1: -2.0,
        z1: 1.0,
        x2: 2.0,
        z2: 1.0,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

fn test_floor() -> Floor {
    Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn collision_world(walls: &[Wall], floors: &[Floor]) -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        walls: walls.to_vec(),
        floors: floors.to_vec(),
        ..Default::default()
    })
}

#[test]
fn world_bounce_reports_first_contact_normal() {
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
    let mut motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));
    let bounce = motion
        .bounce_at_world_surface(&pos, 0.1, &collision_world(&[test_wall()], &[]), &[])
        .expect("projectile should bounce");

    assert!(bounce.normal.dot(Vec3::NEG_Z) > 0.99);
    assert!(bounce.contact.z < 1.0);
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
            .bounce_at_world_surface(&pos, 0.1, &collision_world(&[], &[test_floor()]), &[])
            .is_some()
    );
    assert!(motion.velocity.y < 0.0);
}

#[test]
fn solid_bridges_absorb_projectiles_from_both_sides_instead_of_bouncing() {
    let bridge = FieldId(0);
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: 2.0,
            thickness: 0.1,
            level: 1,
            field: FieldId(0),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    for solid in [false, true] {
        let open: &[FieldId] = if solid { &[] } else { &[bridge] };
        for (y, velocity) in [(4.0, Vec3::NEG_Y * 40.0), (0.0, Vec3::Y * 40.0)] {
            let pos = Position { x: 0.0, y, z: 0.0 };
            let mut motion = test_projectile_motion(velocity);
            assert!(motion.bounce_at_world_surface(&pos, 0.1, &world, &[]).is_none());
            assert_eq!(motion.field_collision_t(&pos, 0.1, &world, open).is_some(), solid);
            let impact = motion.terminate_at_field(&pos, 0.1, &world, open);
            assert_eq!(impact.is_some(), solid);
            if let Some(impact) = impact {
                assert_eq!(impact.field, bridge);
                assert!(impact.normal.dot(velocity) < 0.0);
            }
            assert_eq!(motion.velocity, velocity);
        }
    }
}

// A portal pair on two one-cell carriers sliding by the given travel per
// tick, posed at tick 1, plus extra world walls.
fn moving_projectile_portals(entry_travel: Vec3, exit_travel: Vec3, obstacles: &[Wall]) -> (CollisionWorld, PortalSet) {
    let carrier = Carrier {
        motion: Default::default(),
        initially_on: true,

        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Position::default(),
        to: entry_travel.into(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
        switch: None,
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
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(0, &SwitchState::default());
    carriers.advance(1, &SwitchState::default());
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
        test_fixtures::gameplay_config().portals.size,
        test_fixtures::gameplay_config().player.physics(),
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
    let surface = world.cast_moving_ball(outside, velocity * TICK_SECS, 0.08, &[]);
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
