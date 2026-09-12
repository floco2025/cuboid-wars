use super::*;
use crate::{
    actors::test_kinds::{self, CONTACT},
    map::{CellGrid, EdgeGrid, LevelGrid},
    test_geometry::geometry,
};
use common::protocol::{CarrierId, MapLayout};

#[test]
fn air_home_expands_vertically_and_moves_with_its_spawn_zone() {
    let physics = test_kinds::physics(CONTACT);
    let grid = CarrierGrid::new(
        CarrierId::WORLD,
        geometry(1, 1),
        vec![LevelGrid {
            cells: CellGrid::new(1, 1),
            edges: EdgeGrid::new(1, 1),
            barrier_edges: EdgeGrid::new(1, 1),
        }],
    );
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 1],
        rows: [0, 1],
        kind: CONTACT.into(),
        count: 1,
        respawn_secs: None,
        switch: None,
        switch_inverted: false,
    };
    let pose = CarrierPose::IDENTITY;
    let mut home = AirHome::new(&zone, &grid, physics, 1.5, pose, &[]);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    home.advance(&world, physics, &mut 10000);
    assert!(home.ready());
    let above = Vec3::new(
        home.territory.volume.min.x,
        home.territory.volume.max.y - home.territory.center_height + 0.5,
        home.territory.volume.min.z,
    );
    assert!(home.contains(above, pose));
    assert!(!home.contains(above + Vec3::Y * 2.0, pose));
    let moved = CarrierPose::from_translation(Vec3::new(100.0, 20.0, -10.0));
    assert!(home.contains(moved.transform_point(above), moved));
    assert!(!home.contains(above, moved));
    let mut tight = AirHome::new(&zone, &grid, physics, 0.01, pose, &[]);
    tight.advance(&world, physics, &mut 10000);
    assert!(tight.contains(tight.territory.volume.min + Vec3::splat(0.13), pose));
}

#[test]
fn walls_do_not_change_the_authored_roaming_boundary() {
    use common::protocol::Wall;
    let physics = test_kinds::physics(CONTACT);
    let grid = CarrierGrid::new(
        CarrierId::WORLD,
        geometry(1, 1),
        vec![LevelGrid {
            cells: CellGrid::new(1, 1),
            edges: EdgeGrid::new(1, 1),
            barrier_edges: EdgeGrid::new(1, 1),
        }],
    );
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 1],
        rows: [0, 1],
        kind: CONTACT.into(),
        count: 1,
        respawn_secs: None,
        switch: None,
        switch_inverted: false,
    };
    let edge = grid.geometry.cell_to_world_x(1);
    let target = Vec3::new(edge + 0.9, 1.0, 0.0);
    for blocked in [false, true] {
        let layout = MapLayout {
            walls: if blocked {
                vec![Wall {
                    x1: edge + 0.1,
                    z1: -20.0,
                    x2: edge + 0.1,
                    z2: 20.0,
                    y: -20.0,
                    height: 40.0,
                    width: 0.1,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }]
            } else {
                Vec::new()
            },
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout);
        let mut home = AirHome::new(&zone, &grid, physics, 2.0, CarrierPose::IDENTITY, &[]);
        home.advance(&world, physics, &mut 20000);
        assert!(home.ready());
        assert!(home.contains(target, CarrierPose::IDENTITY));
    }
}

fn sample_home(range: f32) -> AirHome {
    let grid = CarrierGrid::new(
        CarrierId::WORLD,
        geometry(1, 1),
        vec![LevelGrid {
            cells: CellGrid::new(1, 1),
            edges: EdgeGrid::new(1, 1),
            barrier_edges: EdgeGrid::new(1, 1),
        }],
    );
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: range,
        cols: [0, 1],
        rows: [0, 1],
        kind: CONTACT.into(),
        count: 1,
        respawn_secs: None,
        switch: None,
        switch_inverted: false,
    };
    AirHome::new(
        &zone,
        &grid,
        test_kinds::physics(CONTACT),
        range,
        CarrierPose::IDENTITY,
        &[],
    )
}

#[test]
fn large_roam_volumes_have_bounded_samples_spread_across_all_axes() {
    let physics = test_kinds::physics(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let mut home = sample_home(1_000_000.0);
    home.advance(&world, physics, &mut 32);
    let points: Vec<_> = home.points.iter().flatten().copied().collect();
    assert!(points.iter().any(|p| p.y > 200_000.0));
    assert!(points.iter().any(|p| p.y < -200_000.0));
    assert!(!home.ready());
    home.advance(&world, physics, &mut { HOME_SAMPLE_LIMIT });
    assert!(home.ready());
    assert!(home.points.len() <= HOME_SAMPLE_LIMIT);
}

#[test]
fn stationary_nested_maps_do_not_restart_completed_home_samples() {
    use common::{map::Carriers, protocol::Carrier};
    let rest = Vec3::new(100.0, 0.0, 0.0).into();
    let layout = MapLayout {
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: rest,
            to: rest,
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
            switch_inverted: false,
        }],
        ..Default::default()
    };
    let carriers = Carriers::from_layout(&layout);
    assert!(!carriers.is_static());
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut home = sample_home(2.0);
    home.refresh(&world, CarrierPose::IDENTITY, &[]);
    home.advance(&world, test_kinds::physics(CONTACT), &mut { HOME_SAMPLE_LIMIT });
    let points = home.points.clone();
    for _ in 0..5 {
        world.set_carrier_poses(&carriers);
        home.age += 1.0;
        home.refresh(&world, CarrierPose::IDENTITY, &[]);
        assert!(home.ready());
        assert_eq!(home.points, points);
    }
}

#[test]
fn home_refresh_preserves_destinations_until_their_replacements_are_checked() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let physics = test_kinds::physics(CONTACT);
    let mut home = sample_home(2.0);
    home.advance(&world, physics, &mut { HOME_SAMPLE_LIMIT });
    let points = home.points.clone();
    home.age = 1.0;
    let moved = CarrierPose::from_translation(Vec3::X);
    home.refresh(&world, moved, &[]);
    assert!(!home.ready());
    assert_eq!(home.points, points);
    assert!(home.destination(moved, &world, physics, &mut rand::rng()).is_some());
    home.advance(&world, physics, &mut 1);
    assert_eq!(&home.points[1..], &points[1..]);
}

#[test]
fn only_obstacle_motion_near_the_home_restarts_sampling() {
    use common::{
        map::Carriers,
        protocol::{Carrier, PlateState, Wall},
    };
    for near in [false, true] {
        let layout = MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Vec3::new(10.0, 0.0, 0.0).into(),
                to: Vec3::new(if near { 0.0 } else { 20.0 }, 0.0, 0.0).into(),
                travel_ticks: 30,
                pause_ticks: 30,
                phase_ticks: 0,
                switch: None,
                switch_inverted: false,
            }],
            walls: vec![Wall {
                carrier: CarrierId(1),
                level: 0,
                x1: 0.0,
                x2: 0.0,
                z1: -2.0,
                z2: 2.0,
                y: 0.0,
                height: 4.0,
                width: 1.0,
            }],
            ..Default::default()
        };
        let mut carriers = Carriers::from_layout(&layout);
        let mut world = CollisionWorld::from_map_layout(&layout);
        let physics = test_kinds::physics(CONTACT);
        let mut home = sample_home(1.0);
        home.refresh(&world, CarrierPose::IDENTITY, &[]);
        home.advance(&world, physics, &mut { HOME_SAMPLE_LIMIT });
        let points = home.points.clone();
        let revision = world.geometry_revision();
        let initial_bounds = world.geometry_bounds();
        world.set_carrier_poses(&carriers);
        assert_eq!(world.geometry_revision(), revision);
        carriers.advance(30, &PlateState::default());
        world.set_carrier_poses(&carriers);
        assert_ne!(world.geometry_bounds(), initial_bounds);
        home.age = 1.0;
        home.refresh(&world, CarrierPose::IDENTITY, &[]);
        assert_eq!(home.ready(), !near);
        assert_eq!(home.points, points);
        home.advance(&world, physics, &mut { HOME_SAMPLE_LIMIT });
        assert!(home.ready());
        if near {
            assert!(home.points.iter().flatten().count() < points.iter().flatten().count());
        }
    }
}
