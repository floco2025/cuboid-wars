use super::*;
use crate::{
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnZone},
    test_geometry::{LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry},
};
use common::protocol::{Carrier, CarrierId, MapLayout, Wall};

fn empty_layout() -> MapLayout {
    MapLayout::default()
}

fn collision_world(layout: &MapLayout) -> CollisionWorld {
    CollisionWorld::from_map_layout(layout, &common::protocol::BarrierKindTable::default())
}

fn character_physics() -> CharacterPhysicsConfig {
    crate::config::ServerGameplayConfig::load_default()
        .expect("default server gameplay config should load")
        .gameplay_config()
        .player
        .physics()
}

fn actor_config(kind: &str) -> ActorGameplayConfig {
    crate::config::ServerGameplayConfig::load_default()
        .expect("gameplay config rejected")
        .expect_actor(kind)
        .character
        .clone()
}

fn map_config_with_player_spawn(level: u8, col: i32, row: i32) -> MapConfig {
    let mut levels = (0..=level)
        .map(|_| LevelGrid {
            cells: CellGrid::new(2, 2),
            edges: EdgeGrid::new(2, 2),
            barrier_edges: EdgeGrid::new(2, 2),
        })
        .collect::<Vec<_>>();
    levels[usize::from(level)].cells.rows[row as usize][col as usize].has_floor = true;
    MapConfig {
        player_spawn_zones: vec![PlayerSpawnZone {
            carrier: CarrierId::WORLD,
            level,
            cols: [col, col + 1],
            rows: [row, row + 1],
        }],
        ..MapConfig::for_grid(levels, geometry(2, 2))
    }
}

#[test]
fn spawn_position_rejects_other_player_overlap() {
    let layout = empty_layout();
    let collision_world = collision_world(&layout);
    let pos = Position::default();

    assert!(!character_spawn_position_is_clear(
        &pos,
        &collision_world,
        &[pos],
        character_physics()
    ));
}

#[test]
fn spawn_position_rejects_wall_overlap() {
    let mut layout = empty_layout();
    layout.walls.push(Wall {
        x1: -1.0,
        z1: 0.0,
        x2: 1.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    let collision_world = collision_world(&layout);

    assert!(!character_spawn_position_is_clear(
        &Position::default(),
        &collision_world,
        &[],
        character_physics()
    ));
}

#[test]
fn spawn_position_ignores_wall_on_other_level() {
    let mut layout = empty_layout();
    layout.walls.push(Wall {
        x1: -1.0,
        z1: 0.0,
        x2: 1.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level: 1,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    let collision_world = collision_world(&layout);

    assert!(character_spawn_position_is_clear(
        &Position::default(),
        &collision_world,
        &[],
        character_physics()
    ));
}

fn floor_level(cols: i32, rows: i32, floored: &[(i32, i32)]) -> LevelGrid {
    let mut level = LevelGrid {
        cells: CellGrid::new(cols, rows),
        edges: EdgeGrid::new(cols, rows),
        barrier_edges: EdgeGrid::new(cols, rows),
    };
    for &(col, row) in floored {
        level.cells.rows[row as usize][col as usize].has_floor = true;
    }
    level
}

// A 2x2 nested grid resting at `rest`, holding one scuttler zone on its
// (1, 1) cell: a floor, or a ramp, which is never spawnable.
fn nested_zone_fixture(rest: Position, floored: bool) -> (MapConfig, Carriers, ActorSpawnZone) {
    let mut map_config = MapConfig::for_grid(vec![floor_level(2, 2, &[])], geometry(2, 2));
    let mut nested = floor_level(2, 2, &[(1, 1)]);
    if !floored {
        nested.cells.rows[1][1].has_ramp = true;
    }
    map_config
        .grids
        .push(CarrierGrid::new(CarrierId(1), geometry(2, 2), vec![nested]));
    let carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: rest,
            to: rest,
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..MapLayout::default()
    });
    let zone = ActorSpawnZone {
        carrier: CarrierId(1),
        level: 0,
        cols: [1, 2],
        rows: [1, 2],
        kind: "scuttler".to_owned(),
        count: 1,
    };
    (map_config, carriers, zone)
}

#[test]
fn actor_spawn_in_a_nested_zone_goes_through_the_carriers_pose() {
    let collision_world = collision_world(&empty_layout());
    let rest = Position {
        x: 30.0,
        y: LEVEL_HEIGHT,
        z: -10.0,
    };
    let (map_config, carriers, zone) = nested_zone_fixture(rest, true);

    let pos = generate_actor_spawn_position_in_zone(
        &map_config,
        &carriers,
        &zone,
        &collision_world,
        &[],
        &actor_config("scuttler"),
    )
    .expect("floored cell rejected");

    let local = carriers.pose(CarrierId(1)).inverse_transform_position(&pos);
    let geometry = geometry(2, 2);
    assert_eq!(pos.y, LEVEL_HEIGHT);
    assert_eq!(local.y, 0.0);
    assert!(local.x >= geometry.cell_to_world_x(1) && local.x <= geometry.cell_to_world_x(2));
    assert!(local.z >= geometry.cell_to_world_z(1) && local.z <= geometry.cell_to_world_z(2));
}

#[test]
fn actor_spawn_in_a_zone_without_a_spawnable_cell_yields_nothing() {
    let collision_world = collision_world(&empty_layout());
    let (map_config, carriers, zone) = nested_zone_fixture(Position::default(), false);

    assert!(
        generate_actor_spawn_position_in_zone(
            &map_config,
            &carriers,
            &zone,
            &collision_world,
            &[],
            &actor_config("scuttler"),
        )
        .is_none()
    );
}

#[test]
fn immovable_spawn_uses_the_cell_center_in_its_carriers_frame() {
    let world = collision_world(&empty_layout());
    let (map, carriers, zone) = nested_zone_fixture(
        Position {
            x: 30.0,
            y: 4.0,
            z: -10.0,
        },
        true,
    );
    let geometry = map.grid(zone.carrier).geometry;
    let center = Position {
        x: geometry.cell_center_x(1),
        y: 0.0,
        z: geometry.cell_center_z(1),
    };
    let expected = carriers.pose(zone.carrier).transform_position(&center);
    let turret = actor_config("turret");
    for _ in 0..10 {
        assert_eq!(
            generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &[], &turret),
            Some(expected)
        );
    }
    assert!(generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &[expected], &turret).is_none());
}

#[test]
fn immovable_spawn_checks_every_cell_before_reporting_a_full_zone() {
    let world = collision_world(&empty_layout());
    let map = MapConfig::for_grid(
        vec![floor_level(120, 1, &(0..120).map(|c| (c, 0)).collect::<Vec<_>>())],
        geometry(120, 1),
    );
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        cols: [0, 120],
        rows: [0, 1],
        kind: "turret".into(),
        count: 120,
    };
    let geometry = map.root_grid().geometry;
    let centers: Vec<_> = (0..120)
        .map(|col| Position {
            x: geometry.cell_center_x(col),
            y: 0.0,
            z: geometry.cell_center_z(0),
        })
        .collect();
    let turret = actor_config("turret");
    let carriers = Carriers::default();
    assert_eq!(
        generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &centers[..119], &turret),
        Some(centers[119])
    );
    assert!(generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &centers, &turret).is_none());
}

#[test]
fn immovable_spawn_waits_instead_of_shifting_away_from_an_obstructed_center() {
    let geometry = geometry(2, 2);
    let layout = MapLayout {
        walls: vec![Wall {
            x1: geometry.cell_to_world_x(1),
            z1: geometry.cell_center_z(1),
            x2: geometry.cell_to_world_x(2),
            z2: geometry.cell_center_z(1),
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = collision_world(&layout);
    let map = MapConfig::for_grid(vec![floor_level(2, 2, &[(1, 1)])], geometry);
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        cols: [1, 2],
        rows: [1, 2],
        kind: "turret".into(),
        count: 1,
    };
    assert!(
        generate_actor_spawn_position_in_zone(&map, &Carriers::default(), &zone, &world, &[], &actor_config("turret"))
            .is_none()
    );
}

#[test]
fn player_spawn_position_uses_configured_spawn_level() {
    let layout = empty_layout();
    let collision_world = collision_world(&layout);
    let map_config = map_config_with_player_spawn(1, 0, 0);

    let pos = generate_player_spawn_position(
        &map_config,
        &Carriers::default(),
        &collision_world,
        &[],
        character_physics(),
    );

    assert_eq!(pos.y, LEVEL_HEIGHT);
}
