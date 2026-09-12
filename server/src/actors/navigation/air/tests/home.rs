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
        home.volume.min.x,
        home.volume.max.y - home.center_height + 0.5,
        home.volume.min.z,
    );
    assert!(home.contains(above, pose));
    assert!(!home.contains(above + Vec3::Y * 2.0, pose));
    let moved = CarrierPose::from_translation(Vec3::new(100.0, 20.0, -10.0));
    assert!(home.contains(moved.transform_point(above), moved));
    assert!(!home.contains(above, moved));
    let mut tight = AirHome::new(&zone, &grid, physics, 0.01, pose, &[]);
    tight.advance(&world, physics, &mut 10000);
    assert!(tight.contains(tight.volume.min + Vec3::splat(0.13), pose));
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
