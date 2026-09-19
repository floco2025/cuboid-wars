use super::*;
use crate::config::fixtures;
use crate::{
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry},
};
use common::protocol::{Barrier, BarrierKindId, Carrier, CarrierId, Checkpoint, CheckpointKind, MapLayout, Wall};

fn empty_layout() -> MapLayout {
    MapLayout::default()
}

fn collision_world(layout: &MapLayout) -> CollisionWorld {
    CollisionWorld::from_map_layout(layout)
}

fn character_physics() -> CharacterPhysicsConfig {
    fixtures::server_config().gameplay_config().player.physics()
}

fn actor_config(kind: &str) -> ActorGameplayConfig {
    fixtures::server_config().expect_actor(kind).character.clone()
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

fn resting_carrier(rest: Position) -> Carrier {
    Carrier {
        motion: Default::default(),
        switch_inverted: false,

        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: rest,
        to: rest,
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
        switch: None,
    }
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
        carriers: vec![resting_carrier(rest)],
        ..MapLayout::default()
    });
    let zone = ActorSpawnZone {
        switch_inverted: false,

        carrier: CarrierId(1),
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [1, 2],
        rows: [1, 2],
        kind: "scuttler".to_owned(),
        count: vec![1],
        respawn_secs: None,
        beam_in_secs: 0.0,
        switch: None,
        until_checkpoint: None,
        on_checkpoint: Default::default(),
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

    let pos = generate_ground_actor_spawn_position(
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
        generate_ground_actor_spawn_position(
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
            generate_ground_actor_spawn_position(&map, &carriers, &zone, &world, &[], &turret),
            Some(expected)
        );
    }
    assert!(generate_ground_actor_spawn_position(&map, &carriers, &zone, &world, &[expected], &turret).is_none());
}

#[test]
fn immovable_spawn_checks_every_cell_before_reporting_a_full_zone() {
    let world = collision_world(&empty_layout());
    let map = MapConfig::for_grid(
        vec![floor_level(120, 1, &(0..120).map(|c| (c, 0)).collect::<Vec<_>>())],
        geometry(120, 1),
    );
    let zone = ActorSpawnZone {
        switch_inverted: false,

        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 120],
        rows: [0, 1],
        kind: "turret".into(),
        count: vec![120],
        respawn_secs: None,
        beam_in_secs: 0.0,
        switch: None,
        until_checkpoint: None,
        on_checkpoint: Default::default(),
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
        generate_ground_actor_spawn_position(&map, &carriers, &zone, &world, &centers[..119], &turret),
        Some(centers[119])
    );
    assert!(generate_ground_actor_spawn_position(&map, &carriers, &zone, &world, &centers, &turret).is_none());
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
        switch_inverted: false,

        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [1, 2],
        rows: [1, 2],
        kind: "turret".into(),
        count: vec![1],
        respawn_secs: None,
        beam_in_secs: 0.0,
        switch: None,
        until_checkpoint: None,
        on_checkpoint: Default::default(),
    };
    assert!(
        generate_ground_actor_spawn_position(&map, &Carriers::default(), &zone, &world, &[], &actor_config("turret"))
            .is_none()
    );
}

#[test]
fn checkpoint_spawns_follow_carriers_keep_off_the_flag_and_avoid_bodies_and_barriers() {
    let physics = character_physics();
    let rest = Position {
        x: 30.0,
        y: LEVEL_HEIGHT,
        z: -10.0,
    };
    let (map_config, carriers, zone) = nested_zone_fixture(rest, true);
    let geometry = geometry(2, 2);
    let checkpoint = Checkpoint {
        kind: CheckpointKind::Individual,
        number: 1,
        carrier: zone.carrier,
        level: 0,
        cols: zone.cols,
        rows: zone.rows,
        min_x: geometry.cell_to_world_x(1),
        max_x: geometry.cell_to_world_x(2),
        min_z: geometry.cell_to_world_z(1),
        max_z: geometry.cell_to_world_z(2),
        y: 0.0,
    };
    let pose = carriers.pose(zone.carrier);
    let flag = pose.transform_position(&Position {
        x: geometry.cell_center_x(1),
        y: 0.0,
        z: geometry.cell_center_z(1),
    });
    let world = collision_world(&empty_layout());
    let diameter_sq = physics.movement_collider.diameter.powi(2);
    let checkpoints = std::slice::from_ref(&checkpoint);
    let mut occupied = Vec::new();
    for _ in 0..40 {
        let pos =
            generate_checkpoint_spawn_position(&map_config, &carriers, checkpoints, 1, &world, &occupied, physics)
                .expect("clear checkpoint rejected");
        let local = pose.inverse_transform_position(&pos);
        assert_eq!(pos.y, LEVEL_HEIGHT);
        assert!(
            (checkpoint.min_x..=checkpoint.max_x).contains(&local.x)
                && (checkpoint.min_z..=checkpoint.max_z).contains(&local.z),
            "{local:?} lies outside the checkpoint"
        );
        assert!(
            pos.horizontal_distance_sq(&flag) >= diameter_sq - 1e-3,
            "{pos:?} stands in the flag at {flag:?}"
        );
        for other in &occupied {
            assert!(pos.horizontal_distance_sq(other) >= diameter_sq - 1e-3);
        }
        if occupied.is_empty() {
            occupied.push(pos);
        }
    }

    let (ramped, carriers, _) = nested_zone_fixture(rest, false);
    assert!(generate_checkpoint_spawn_position(&ramped, &carriers, checkpoints, 1, &world, &[], physics).is_none());
    assert!(
        generate_checkpoint_spawn_position(&map_config, &carriers, checkpoints, 2, &world, &[], physics).is_none(),
        "a number with no rectangle has no spot"
    );

    let barred = MapLayout {
        carriers: vec![resting_carrier(rest)],
        barriers: vec![Barrier {
            id: Default::default(),

            switch: None,
            switch_inverted: false,

            x1: checkpoint.min_x,
            z1: geometry.cell_center_z(1),
            x2: checkpoint.max_x,
            z2: geometry.cell_center_z(1),
            y: 0.0,
            height: 3.0,
            width: geometry.cell_size(),
            kind: BarrierKindId(0),
            level: 0,
            levels: 1,
            carrier: zone.carrier,
        }],
        ..default()
    };
    let mut world = collision_world(&barred);
    world.set_carrier_poses(&carriers);
    assert!(generate_checkpoint_spawn_position(&map_config, &carriers, checkpoints, 1, &world, &[], physics).is_none());
}
