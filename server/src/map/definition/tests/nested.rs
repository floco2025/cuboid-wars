use super::*;

fn motion(level: u32, from: [i32; 2], to: [i32; 2], to_level: u32) -> MotionDef {
    MotionDef {
        switch_inverted: false,

        level,
        from,
        to,
        to_level: Some(to_level),
        travel_secs: 2.0,
        pause_secs: 0.5,
        phase_secs: 0.0,
        from_nudge: [0.0; 3],
        to_nudge: [0.0; 3],
        switch: None,
    }
}

fn nested(map: &str, level: u32, from: [i32; 2], to: [i32; 2], to_level: u32) -> NestedMapDef {
    NestedMapDef {
        map: map.into(),
        motion: motion(level, from, to, to_level),
    }
}

fn barrier_corridor() -> MapDef {
    let mut map = map_with_zones(
        3,
        vec![level(vec![[0, 0], [1, 0], [2, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for (col, kind) in [(1, "red"), (2, "blue")] {
        map.levels[0].barriers.push(BarrierDef {
            switch: (kind == "red").then(|| "red".into()),
            switch_inverted: false,

            c0: col,
            r0: 0,
            c1: col,
            r1: 1,
            kind: kind.into(),
        });
    }
    map
}

fn red_barrier_plate() -> PressurePlateDef {
    PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        switch: "red".into(),
    }
}

fn assert_only_plate_barrier_allows_a_route(config: &MapConfig, carrier: CarrierId) {
    let grid = config.grid(carrier);
    let nav = NavGraph::new(grid);
    let position = |col| Position {
        x: grid.geometry.cell_center_x(col),
        y: grid.geometry.level_y(0),
        z: grid.geometry.cell_center_z(0),
    };
    assert!(
        nav.engagement_route(&[], &position(0), &position(1), 0.2, 0.2)
            .is_some(),
        "the red barrier's controlling plate must allow an actor route"
    );
    assert!(
        nav.engagement_route(&[], &position(1), &position(2), 0.2, 0.2)
            .is_none(),
        "the blue barrier has no controlling plate and must block actor routes"
    );
}

#[test]
fn a_nested_plate_allows_actor_routes_through_parent_barriers() {
    let mut root = barrier_corridor();
    root.nested_maps.push(nested("switch", 0, [0, 1], [1, 1], 0));
    let mut switch = host(Vec::new());
    switch.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_with(
        &root,
        &tree(vec![("switch", switch)]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("nested plate map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId::WORLD);
}

#[test]
fn a_parent_plate_allows_actor_routes_through_nested_barriers() {
    let mut root = host(vec![nested("corridor", 0, [0, 1], [1, 1], 0)]);
    root.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_with(
        &root,
        &tree(vec![("corridor", barrier_corridor())]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("nested barrier map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId(1));
}

#[test]
fn a_deeply_nested_plate_allows_actor_routes_through_a_siblings_barriers() {
    let root = host(vec![
        nested("corridor", 0, [0, 1], [1, 1], 0),
        nested("middle", 0, [0, 3], [1, 3], 0),
    ]);
    let middle = host(vec![nested("switch", 0, [0, 1], [1, 1], 0)]);
    let mut switch = host(Vec::new());
    switch.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_with(
        &root,
        &tree(vec![
            ("corridor", barrier_corridor()),
            ("middle", middle),
            ("switch", switch),
        ]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("deeply nested plate map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId(1));
}

#[test]
fn firework_plate_does_not_open_any_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        switch: FIREWORKS.into(),
    });

    let (layout, config) = compile_with(&map_def, &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    assert!(
        config.root_grid().levels[0].barrier_edges.vertical[0][1],
        "a firework plate opens no barrier kind for nav"
    );
    let fireworks = switch_id(&three_kind_table(), &no_bridges(), FIREWORKS);
    assert_eq!(config.pressure_plates[0].switch, fireworks);
    assert_eq!(layout.pressure_plates[0].switch, fireworks);
}

// A 3x2 room with a floor on every cell, a wall along its north edge, one
// gold, one firework plate, a player zone on its first cell, and an actor
// zone, on two storeys.
fn room() -> MapDef {
    let mut map_def = map_with_zones(
        3,
        vec![
            level(vec![[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]]),
            level(vec![[0, 0]]),
        ],
        vec![actor_zone(0, 1, 1)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.grid_rows = 2;
    map_def.levels[0].walls.push(WallDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        materials: FaceMaterials::uniform("test"),
    });
    map_def.items.push(ItemDef {
        level: 0,
        col: 2,
        row: 1,
        item_type: "gold".into(),
        kind: None,
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 1,
        row: 1,
        switch: FIREWORKS.into(),
    });
    map_def
}

// A 6x6 host with one floor, nesting `entries`.
fn host(entries: Vec<NestedMapDef>) -> MapDef {
    let mut map_def = map_with_zones(
        6,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.nested_maps = entries;
    map_def
}

fn tree(maps: Vec<(&str, MapDef)>) -> LoadedMaps {
    maps.into_iter().map(|(name, def)| (name.to_owned(), def)).collect()
}

fn compile_host(host: &MapDef, nested: &LoadedMaps) -> (common::protocol::MapLayout, crate::map::MapConfig) {
    compile_with(host, nested, &empty_kind_table(), &no_bridges()).expect("host failed to compile")
}

#[test]
fn validation_rejects_nested_map_with_path_unsafe_name() {
    let map_def = host(vec![nested("../secret", 0, [2, 2], [2, 2], 0)]);
    let error = validate_map(&map_def).expect_err("path-unsafe nested name accepted");
    assert!(error.to_string().contains("nested_maps[0]"), "{error}");
}

#[test]
fn validation_rejects_nested_map_anchor_outside_the_grid() {
    let map_def = host(vec![nested("room", 0, [6, 2], [2, 2], 0)]);
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_nested_map_level_out_of_range() {
    let map_def = host(vec![nested("room", 3, [2, 2], [2, 2], 3)]);
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_non_positive_nested_map_speed() {
    let mut map_def = host(vec![nested("room", 0, [2, 2], [4, 2], 0)]);
    map_def.nested_maps[0].motion.travel_secs = 0.0;
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_a_non_finite_nudge() {
    let mut map_def = host(vec![nested("room", 0, [2, 2], [4, 2], 0)]);
    map_def.nested_maps[0].motion.to_nudge = [0.0, f32::NAN, 0.0];
    let error = validate_map(&map_def).expect_err("non-finite nudge accepted");
    assert!(format!("{error:#}").contains("to_nudge"), "{error:#}");
}

#[test]
fn nudges_displace_each_end_by_wall_widths_across_and_floor_thicknesses_up() {
    use common::protocol::CarrierId;

    let mut entry = nested("room", 0, [1, 3], [5, 3], 0);
    entry.motion.from_nudge = [1.0, 0.0, 0.0];
    entry.motion.to_nudge = [0.0, -2.0, 3.0];
    let plain = host(vec![nested("room", 0, [1, 3], [5, 3], 0)]);
    let (plain_layout, _) = compile_host(&plain, &tree(vec![("room", room())]));
    let (layout, _) = compile_host(&host(vec![entry]), &tree(vec![("room", room())]));
    let (plain_from, plain_to) = (
        Vec3::from(plain_layout.carriers[0].from),
        Vec3::from(plain_layout.carriers[0].to),
    );
    let carrier = layout.carriers[0];

    assert_eq!(carrier.parent, CarrierId::WORLD);
    let expected_from = plain_from + Vec3::X * WALL_THICKNESS;
    let expected_to = plain_to + Vec3::new(0.0, -2.0 * FLOOR_THICKNESS, 3.0 * WALL_THICKNESS);
    assert!(
        (Vec3::from(carrier.from) - expected_from).length() < 1e-5,
        "from {:?}",
        carrier.from
    );
    assert!(
        (Vec3::from(carrier.to) - expected_to).length() < 1e-5,
        "to {:?}",
        carrier.to
    );
}

#[test]
fn nudges_default_to_zero() {
    let entry: NestedMapDef =
        serde_json::from_str(r#"{"map": "room", "level": 0, "from": [1, 1], "to": [3, 1], "travel_secs": 2.0}"#)
            .expect("entry without nudges rejected");
    assert_eq!((entry.motion.from_nudge, entry.motion.to_nudge), ([0.0; 3], [0.0; 3]));
}

#[test]
fn travel_time_sets_the_travel_ticks_whatever_the_distance() {
    let mut short = nested("room", 0, [1, 3], [2, 3], 0);
    short.motion.travel_secs = 1.5;
    let mut long = nested("room", 0, [1, 3], [5, 3], 0);
    long.motion.travel_secs = 1.5;
    let (layout, _) = compile_host(&host(vec![short, long]), &tree(vec![("room", room())]));
    assert_eq!(layout.carriers[0].travel_ticks, 45);
    assert_eq!(layout.carriers[1].travel_ticks, 45);
}

#[test]
fn validation_accepts_a_stationary_nested_map() {
    let map_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    validate_map(&map_def).expect("a room placed once was rejected");
}

#[test]
fn validation_accepts_a_file_without_player_spawn_zones() {
    let mut map_def = room();
    map_def.player_spawn_zones.clear();
    validate_map(&map_def).expect("nested geometry without spawn zones was rejected");
}

#[test]
fn validation_rejects_two_nested_maps_starting_on_one_cell() {
    let map_def = host(vec![
        nested("room", 0, [2, 2], [4, 2], 0),
        nested("room", 0, [2, 2], [2, 4], 0),
    ]);
    let error = validate_map(&map_def).expect_err("duplicate start cell accepted");
    assert!(error.to_string().contains("duplicates"), "{error}");
}

#[test]
fn nested_cell_zero_lands_on_the_parent_anchor_cell() {
    use common::{map::MapGeometry, protocol::CarrierId};

    let host_def = host(vec![nested("room", 1, [2, 3], [4, 3], 1)]);
    let (layout, _) = compile_host(&host_def, &tree(vec![("room", room())]));
    let carrier = layout.carriers[0];
    let parent = MapGeometry::new(6, 6, sizes());
    let child = MapGeometry::new(3, 2, sizes());

    assert_eq!(carrier.parent, CarrierId::WORLD);
    assert!((carrier.from.x + child.cell_center_x(0) - parent.cell_center_x(2)).abs() < 1e-5);
    assert!((carrier.from.z + child.cell_center_z(0) - parent.cell_center_z(3)).abs() < 1e-5);
    assert!((carrier.from.y - parent.level_y(1)).abs() < 1e-5);
    assert!((carrier.to.x + child.cell_center_x(0) - parent.cell_center_x(4)).abs() < 1e-5);
    assert_eq!((carrier.level, carrier.levels), (1, 0));
}

#[test]
fn nested_records_stay_in_their_own_frame_and_carry_their_id() {
    use common::{map::MapGeometry, protocol::CarrierId};

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (layout, _) = compile_host(&host_def, &tree(vec![("room", room())]));
    let child = MapGeometry::new(3, 2, sizes());
    let room_walls: Vec<_> = layout
        .walls
        .iter()
        .filter(|wall| wall.carrier == CarrierId(1))
        .collect();
    assert_eq!(room_walls.len(), 1);
    // The room's north wall runs along its own grid line z = row 0, not the host's.
    assert!(
        (room_walls[0].z1 - child.cell_to_world_z(0)).abs() < 1e-5,
        "wall at {:?}",
        room_walls[0]
    );
    assert!(layout.floors.iter().any(|floor| floor.carrier == CarrierId(1)));
    assert!(layout.floors.iter().any(|floor| floor.carrier.is_world()));
    assert_eq!(layout.pressure_plates.len(), 1);
    assert_eq!(layout.pressure_plates[0].carrier, CarrierId(1));
    assert!((layout.pressure_plates[0].center_x - child.cell_center_x(1)).abs() < 1e-5);
}

#[test]
fn nested_kinds_resolve_against_the_root_tables_and_an_unknown_kind_names_the_nested_map() {
    let mut keyed_room = room();
    keyed_room.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let nested_maps = tree(vec![("room", keyed_room)]);

    let (layout, _) = compile_with(&host_def, &nested_maps, &red_only_kind_table(), &no_bridges())
        .expect("a nested barrier of a root kind failed to compile");
    assert_eq!(layout.barriers.len(), 1);

    let error = compile_with(&host_def, &nested_maps, &empty_kind_table(), &no_bridges())
        .expect_err("an unknown nested kind compiled");
    assert!(format!("{error:#}").contains("nested map \"room\""), "{error:#}");
}

#[test]
fn a_doubly_nested_carrier_is_parented_to_its_nesting_carrier_and_ids_come_parent_first() {
    use common::protocol::CarrierId;

    let mut middle = room();
    middle.nested_maps.push(nested("inner", 0, [1, 0], [1, 0], 0));
    let host_def = host(vec![
        nested("middle", 0, [2, 2], [2, 2], 0),
        nested("inner", 0, [0, 4], [0, 4], 0),
    ]);
    let (layout, config) = compile_host(&host_def, &tree(vec![("middle", middle), ("inner", room())]));

    // middle = 1, its inner = 2, the host's own inner = 3.
    assert_eq!(layout.carriers.len(), 3);
    assert_eq!(layout.carriers[0].parent, CarrierId::WORLD);
    assert_eq!(layout.carriers[1].parent, CarrierId(1));
    assert_eq!(layout.carriers[2].parent, CarrierId::WORLD);
    assert_eq!(config.grids.len(), 4);
    for (index, grid) in config.grids.iter().enumerate() {
        assert_eq!(grid.carrier, CarrierId(index as u16));
    }
    assert_eq!(config.grid(CarrierId(2)).geometry.grid_cols, 3);
}

#[test]
fn nested_actor_spawn_zones_carry_their_carrier() {
    use common::protocol::CarrierId;

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (_, config) = compile_host(&host_def, &tree(vec![("room", room())]));
    assert_eq!(config.actor_spawn_zones.len(), 1);
    assert_eq!(config.actor_spawn_zones[0].carrier, CarrierId(1));
    assert_eq!(config.actor_spawn_zones[0].cols, [1, 2]);
    assert_eq!(config.actor_spawn_zones[0].rows, [1, 2]);
}

#[test]
fn nested_player_spawn_zones_items_and_plates_carry_their_carrier() {
    use common::protocol::CarrierId;

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (_, config) = compile_host(&host_def, &tree(vec![("room", room())]));
    assert_eq!(config.player_spawn_zones.len(), 2);
    assert_eq!(config.player_spawn_zones[1].carrier, CarrierId(1));
    assert_eq!(config.placed_items.len(), 1);
    assert_eq!(config.placed_items[0].carrier, CarrierId(1));
    assert_eq!(config.pressure_plates.len(), 1);
    assert_eq!(config.pressure_plates[0].carrier, CarrierId(1));
}

#[test]
fn a_nested_map_names_the_switch_that_runs_its_carrier() {
    let mut entry = nested("room", 0, [2, 2], [2, 5], 0);
    entry.motion.switch = Some(FIREWORKS.into());
    let host_def = host(vec![entry]);
    let (layout, _) = compile_host(&host_def, &tree(vec![("room", room())]));
    let fireworks = switch_id(&empty_kind_table(), &no_bridges(), FIREWORKS);
    assert_eq!(layout.carriers[0].switch, Some(fireworks));

    let mut entry = nested("room", 0, [2, 2], [2, 5], 0);
    entry.motion.switch = Some("void".into());
    let host_def = host(vec![entry]);
    let error = compile_with(
        &host_def,
        &tree(vec![("room", room())]),
        &empty_kind_table(),
        &no_bridges(),
    )
    .expect_err("an unknown carrier switch compiled");
    assert!(format!("{error:#}").contains("unknown switch"), "{error:#}");

    let mut plateless = room();
    plateless.pressure_plates.clear();
    let mut entry = nested("room", 0, [2, 2], [2, 5], 0);
    entry.motion.switch = Some(FIREWORKS.into());
    let host_def = host(vec![entry]);
    let error = compile_with(
        &host_def,
        &tree(vec![("room", plateless)]),
        &empty_kind_table(),
        &no_bridges(),
    )
    .expect_err("a carrier on an unplated switch compiled");
    assert!(
        format!("{error:#}").contains("operated by no pressure plate"),
        "{error:#}"
    );
}
