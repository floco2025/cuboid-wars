use super::*;
use crate::test_fixtures::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, gameplay_config, geometry};
use common::{
    config::MissilesConfig,
    constants::TICK_SECS,
    protocol::{BarrierKindTable, Carrier, CarrierId, Floor, MapLayout, PlateState, PlayerId, Wall},
};
use std::f32::consts::FRAC_PI_4;

fn info() -> MissileFlight {
    MissileFlight::new(PlayerId(0), None, 0.0, 10.0)
}

fn config() -> MissilesConfig {
    gameplay_config().missiles
}

fn map(cols: i32, rows: i32, levels: usize) -> AirGraph {
    AirGraph {
        grids: vec![super::super::air_graph::AirGrid {
            carrier: CarrierId::WORLD,
            geometry: geometry(cols, rows),
            layers: levels as i32 + 1,
        }],
    }
}

fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        width: WALL_THICKNESS,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn world(layout: &MapLayout) -> CollisionWorld {
    CollisionWorld::from_map_layout(layout, &BarrierKindTable::default())
}

#[test]
fn a_nearby_corner_is_not_skipped_when_the_next_leg_is_blocked() {
    let world = world(&MapLayout {
        walls: vec![wall(1.0, -1.0, 1.0, 1.6)],
        ..default()
    });
    let corner = Vec3::new(0.0, 2.0, 1.0);
    let mut path = VecDeque::from([corner, Vec3::new(2.0, 2.0, 2.0)]);
    advance_waypoints(&mut path, Vec3::new(0.0, 2.0, 0.0), &world, &[], MISSILE_RADIUS);
    assert_eq!(path.len(), 2);
    assert_eq!(path.front(), Some(&corner));
}

#[test]
fn a_wall_moving_across_a_cached_route_triggers_an_immediate_replan() {
    let graph = map(4, 4, 2);
    let layout = MapLayout {
        walls: vec![Wall {
            carrier: CarrierId(1),
            ..wall(0.0, -5.0, 0.0, 5.0)
        }],
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::from(Vec3::X * 20.0),
            to: Position::default(),
            travel_ticks: 60,
            pause_ticks: 30,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    };
    let mut carriers = Carriers::from_layout(&layout);
    let mut world = world(&layout);
    let origin = Vec3::new(-3.0, 1.5, 0.0);
    let target = Vec3::new(3.0, 1.5, 0.0);
    let mut info = info();
    info.path = graph
        .path(&carriers, &world, &[], origin, target, MISSILE_RADIUS, 1.0)
        .expect("initial route missing");
    info.path_target = Some(target);
    info.path_retry_timer = 0.4;
    carriers.advance(60, &PlateState::default());
    world.set_carrier_poses(&carriers);
    assert!(!route_clear(
        &info.path,
        origin,
        target,
        &world,
        &[],
        MISSILE_RADIUS,
        1.0
    ));
    assert!(
        route_objective(
            &mut info,
            &graph,
            &carriers,
            &world,
            &[],
            origin,
            target,
            MISSILE_RADIUS,
            1.0,
            TICK_SECS
        )
        .is_some()
    );
    assert!(route_clear(
        &info.path,
        origin,
        target,
        &world,
        &[],
        MISSILE_RADIUS,
        1.0
    ));
    assert_eq!(info.path_retry_timer, MISSILE_PATH_RETRY_SECS);
}

#[test]
fn failed_routes_obey_the_retry_timer() {
    let graph = map(2, 1, 1);
    let world = world(&MapLayout::default());
    let mut info = info();
    let target = Vec3::X;
    info.path_target = Some(target);
    info.path_retry_timer = 0.4;
    assert!(
        route_objective(
            &mut info,
            &graph,
            &Carriers::default(),
            &world,
            &[],
            Vec3::ZERO,
            target,
            MISSILE_RADIUS,
            1.0,
            TICK_SECS
        )
        .is_none()
    );
    assert!(info.path_retry_timer < 0.4);
    assert!(info.path_retry_timer > 0.3);
}

#[test]
fn lead_pursuit_does_not_aim_through_a_wall_beside_a_visible_target() {
    let graph = map(8, 8, 2);
    let world = world(&MapLayout {
        walls: vec![wall(2.0, 1.0, 2.0, 12.0)],
        ..default()
    });
    let origin = Vec3::new(0.0, 2.0, 0.0);
    let target = Vec3::new(0.0, 2.0, 10.0);
    let mut info = info();
    info.last_target_center = Some(target - Vec3::X * (10.0 * TICK_SECS));
    let config = MissilesConfig {
        weave_strength: 0.0,
        ..config()
    };
    let velocity = guided_velocity(
        &mut info,
        &config,
        &graph,
        &Carriers::default(),
        &world,
        &[],
        origin,
        target,
        Vec3::Z * 16.0,
        16.0,
        TICK_SECS,
    );
    assert!(velocity.abs_diff_eq(Vec3::Z * 16.0, 1e-4));
}

#[test]
fn a_cached_route_is_invalid_when_cover_blocks_its_terminal_approach() {
    let origin = Vec3::new(3.0, 1.0, 0.0);
    let target = Vec3::new(WALL_THICKNESS / 2.0 + 0.26, 1.0, 0.0);
    let mut layout = MapLayout {
        walls: vec![wall(0.0, -2.0, 0.0, 2.0)],
        ..default()
    };
    let end = terminal_approach(&world(&layout), &[], origin, target, MISSILE_RADIUS, 1.0)
        .expect("exposed terminal approach missing");
    let path = VecDeque::from([end]);
    assert!(route_clear(
        &path,
        origin,
        target,
        &world(&layout),
        &[],
        MISSILE_RADIUS,
        1.0
    ));
    layout.walls.push(Wall {
        width: 0.1,
        ..wall(0.9, -2.0, 0.9, 2.0)
    });
    let world = world(&layout);
    assert!(sweep_clear(&world, &[], origin, end - origin, MISSILE_RADIUS));
    assert!(!route_clear(&path, origin, target, &world, &[], MISSILE_RADIUS, 1.0));
}

#[test]
fn proximity_fuse_requires_clear_flight_and_blast_paths() {
    let world = world(&MapLayout {
        walls: vec![wall(0.0, -4.0, 0.0, 4.0)],
        ..default()
    });
    let target = Vec3::new(WALL_THICKNESS / 2.0 + 0.26, 1.0, 0.0);
    assert_eq!(
        proximity_detonation(&world, &[], Vec3::new(1.0, 1.0, -2.0), Vec3::Z * 4.0, target, 1.0),
        Some(Vec3::new(1.0, 1.0, 0.0))
    );
    assert!(proximity_detonation(&world, &[], Vec3::new(-0.5, 1.0, 0.0), Vec3::Z * 0.3, target, 1.0).is_none());
    assert!(proximity_detonation(&world, &[], Vec3::new(0.8, 1.0, 0.0), Vec3::NEG_X, target, 1.0).is_none());
}

#[test]
fn missiles_reach_an_exposed_target_too_close_to_a_wall_for_their_radius() {
    let graph = map(20, 20, 1);
    let world = world(&MapLayout {
        walls: vec![Wall {
            width: 0.4,
            height: 2.0,
            ..wall(0.0, -34.0, 0.0, 34.0)
        }],
        floors: vec![Floor {
            x1: -34.0,
            z1: -34.0,
            x2: 34.0,
            z2: 34.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    });
    let config = config();
    let target = Vec3::new(0.46, 0.815, 0.0);
    let mut origin = Vec3::new(2.0129144, 1.62, -5.795555);
    assert!(world.attack_path_clear(origin, target, &[]));
    assert!(!sweep_clear(&world, &[], origin, target - origin, MISSILE_RADIUS));
    let aim = (target - origin).normalize();
    let axis = Quat::from_axis_angle(aim, FRAC_PI_4) * aim.any_orthonormal_vector();
    let mut velocity = Quat::from_axis_angle(axis, 22.5_f32.to_radians()) * aim * 16.0;
    let mut info = info();
    info.weave_phase = 0.7;
    for _ in 0..300 {
        info.lifetime_timer -= TICK_SECS;
        velocity = guided_velocity(
            &mut info,
            &config,
            &graph,
            &Carriers::default(),
            &world,
            &[],
            origin,
            target,
            velocity,
            16.0,
            TICK_SECS,
        );
        if let Some(closest) = proximity_detonation(
            &world,
            &[],
            origin,
            velocity * TICK_SECS,
            target,
            config.proximity_fuse_distance,
        ) {
            assert!(sweep_clear(&world, &[], origin, closest - origin, MISSILE_RADIUS));
            assert!(world.attack_path_clear(closest, target, &[]));
            return;
        }
        assert!(sweep_clear(&world, &[], origin, velocity * TICK_SECS, MISSILE_RADIUS));
        origin += velocity * TICK_SECS;
    }
    panic!("missile failed to reach exposed target, ended at {origin}");
}

#[test]
fn missiles_reach_targets_inside_a_moving_room_without_clipping_its_shell() {
    let mut map = map(12, 12, 3);
    let room_size = geometry(3, 3);
    let mut room_grid = self::map(3, 3, 2).grids.remove(0);
    room_grid.carrier = CarrierId(1);
    map.grids.push(room_grid);
    let graph = map;
    let half = room_size.width() / 2.0;
    let door = room_size.cell_size() / 2.0;
    let layout = MapLayout {
        walls: [
            wall(-half, -half, half, -half),
            wall(-half, half, half, half),
            wall(half, -half, half, half),
            wall(-half, -half, -half, -door),
            wall(-half, door, -half, half),
            wall(0.0, -half, 0.0, door),
        ]
        .into_iter()
        .map(|wall| Wall {
            carrier: CarrierId(1),
            ..wall
        })
        .collect(),
        floors: [0.0, LEVEL_HEIGHT]
            .into_iter()
            .map(|y| Floor {
                x1: -half,
                z1: -half,
                x2: half,
                z2: half,
                y,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId(1),
            })
            .collect(),
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position::from(Vec3::new(-0.75, 0.4, -0.45)),
            to: Position::from(Vec3::new(0.75, 2.0, 0.65)),
            travel_ticks: 180,
            pause_ticks: 30,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    };
    let config = config();
    for first_tick in [0, 90, 210] {
        for side in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            let mut carriers = Carriers::from_layout(&layout);
            carriers.advance(first_tick, &PlateState::default());
            let mut world = world(&layout);
            let local_target = Vec3::new(2.0, 1.1, 1.0);
            let target = carriers.pose(CarrierId(1)).transform_point(local_target);
            let mut origin = target + side * 10.0;
            let mut velocity = -side * 16.0;
            let mut info = info();
            let mut reached = false;
            for tick in first_tick..first_tick + 300 {
                carriers.advance(tick, &PlateState::default());
                world.set_carrier_poses(&carriers);
                let target = carriers.pose(CarrierId(1)).transform_point(local_target);
                info.lifetime_timer -= TICK_SECS;
                velocity = guided_velocity(
                    &mut info,
                    &config,
                    &graph,
                    &carriers,
                    &world,
                    &[],
                    origin,
                    target,
                    velocity,
                    16.0,
                    TICK_SECS,
                );
                if let Some(closest) = proximity_detonation(
                    &world,
                    &[],
                    origin,
                    velocity * TICK_SECS,
                    target,
                    config.proximity_fuse_distance,
                ) {
                    assert!(sweep_clear(&world, &[], origin, closest - origin, MISSILE_RADIUS));
                    reached = true;
                    break;
                }
                assert!(
                    sweep_clear(&world, &[], origin, velocity * TICK_SECS, MISSILE_RADIUS),
                    "first tick {first_tick}, side {side}, tick {tick}: collision at {origin}, velocity {velocity}, route {:?}",
                    info.path
                );
                origin += velocity * TICK_SECS;
                assert!(
                    !info.watchdog.tick_3d(
                        &Position::from(origin),
                        TICK_SECS,
                        MISSILE_STALL_PROGRESS_DISTANCE,
                        config.stall_secs
                    ),
                    "missile stalled at {origin}"
                );
            }
            assert!(
                reached,
                "first tick {first_tick}, side {side}: failed to reach target, ended at {origin}"
            );
        }
    }
}

#[test]
fn a_missile_skimming_geometry_still_fuses_on_its_target() {
    let world = world(&MapLayout {
        walls: vec![wall(0.0, -4.0, 0.0, 4.0)],
        ..default()
    });
    // Hugging the wall face: the missile's own centre is within its radius of it.
    let origin = Vec3::new(WALL_THICKNESS / 2.0 + 0.2, 1.0, 0.0);
    let target = Vec3::new(1.0, 1.0, 0.0);
    let travel = Vec3::X * 0.5;
    assert!(!sweep_clear(&world, &[], origin, travel, MISSILE_RADIUS));
    assert_eq!(
        proximity_detonation(&world, &[], origin, travel, target, 1.0),
        Some(origin + travel)
    );
}

#[test]
fn a_route_whose_waypoints_went_unreachable_retries_on_the_next_tick() {
    let graph = map(4, 4, 2);
    let world = world(&MapLayout {
        walls: vec![wall(0.0, -4.0, 0.0, 4.0)],
        ..default()
    });
    let origin = Vec3::new(WALL_THICKNESS / 2.0 + 0.2, 1.0, 0.0);
    let target = Vec3::new(1.0, 1.0, 0.0);
    let mut info = info();
    info.path_target = Some(target);
    assert!(
        route_objective(
            &mut info,
            &graph,
            &Carriers::default(),
            &world,
            &[],
            origin,
            target,
            MISSILE_RADIUS,
            1.0,
            TICK_SECS
        )
        .is_none()
    );
    assert!(info.path.is_empty());
    assert_eq!(info.path_retry_timer, 0.0);
}
