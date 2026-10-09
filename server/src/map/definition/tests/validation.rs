use super::*;

#[test]
fn spawn_volumes_validate_all_levels_and_allow_zero_roam_extension() {
    let mut map = map_with_zones(
        4,
        vec![level(Vec::new()), level(Vec::new())],
        vec![actor_zone(0, 0, 0)],
        Vec::new(),
    );
    map.actor_spawn_zones[0].levels = 2;
    validate_map(&map).expect("valid spawn volumes rejected");
    for distance in [-1.0, f32::NAN, f32::INFINITY] {
        map.actor_spawn_zones[0].roam_distance = distance;
        assert!(validate_map(&map).is_err());
    }
    map.actor_spawn_zones[0].roam_distance = 0.0;
    for span in [0, 3, u32::MAX] {
        map.actor_spawn_zones[0].levels = span;
        assert!(validate_map(&map).is_err());
        map.actor_spawn_zones[0].levels = 2;
    }
}

#[test]
fn actor_zones_may_cover_floorless_inaccessible_and_ramp_cells() {
    // Flying kinds need no floor, and the spawn picker skips the cells that
    // are not spawnable (`Cell::is_spawnable`), so a zone may brush any cell.
    for map_def in [
        map_with_zones(
            4,
            vec![level(vec![[0, 0]]), level(vec![[1, 0]])],
            vec![actor_zone(1, 0, 0)],
            Vec::new(),
        ),
        map_with_zones(
            4,
            vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
            vec![actor_zone(0, 1, 0)],
            Vec::new(),
        ),
        map_with_zones(
            4,
            vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
            vec![actor_zone(1, 0, 0)],
            vec![ramp([0, 1], [0, 2], RampDirection::South, 1)],
        ),
    ] {
        validate_map(&map_def).expect("a zone over an unspawnable cell rejected");
    }
}

#[test]
fn a_barrier_edge_may_not_repeat_or_overlap_a_wall_in_either_direction() {
    let barrier = |c0, c1, field: &str| BarrierDef {
        c0,
        r0: 0,
        c1,
        r1: 0,
        field: field.into(),
    };
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].barriers = vec![barrier(0, 1, "red"), barrier(1, 0, "green")];
    let err = validate_map(&map_def).expect_err("duplicate barrier edge accepted");
    assert!(err.to_string().contains("duplicates another barrier"), "{err}");

    map_def.levels[0].barriers = vec![barrier(1, 0, "blue")];
    map_def.levels[0].walls.push(WallDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        materials: FaceMaterials::uniform("test"),
    });
    let err = validate_map(&map_def).expect_err("barrier on a wall edge accepted");
    assert!(err.to_string().contains("overlaps a wall"), "{err}");
}

#[test]
fn a_light_bridge_may_not_sit_on_a_floor_a_ramp_or_another_bridge() {
    let err = validate_map(&map_with_bridges(&[[0, 0]])).expect_err("a bridge on a floor accepted");
    assert!(err.to_string().contains("sits on a floor"), "{err}");

    let err = validate_map(&map_with_bridges(&[[1, 0], [1, 0]])).expect_err("duplicate bridge cells accepted");
    assert!(err.to_string().contains("duplicates another light bridge"), "{err}");

    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![ramp([1, 3], [0, 1], RampDirection::East, 0)],
    );
    map_def.levels[0].light_bridges.push(bridge_def(1, 0));
    let err = validate_map(&map_def).expect_err("a bridge on a ramp accepted");
    assert!(err.to_string().contains("sits on a ramp"), "{err}");
}

#[test]
fn validate_rejects_plate_conflicts_with_bridges_and_other_plates() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 1,
        row: 0,
        switch: FIREWORKS.into(),
    });

    let err = validate_map(&map_def).expect_err("a plate on a bridge must fail");
    assert!(err.to_string().contains("sits on a light bridge"), "got: {err}");

    map_def.levels[0].light_bridges.clear();
    map_def.levels[0].floors.push(floor_def(1, 0));
    map_def.levels.push(level(vec![[1, 0]]));
    for switch in ["red", "blue", "skyway", FIREWORKS] {
        map_def.pressure_plates[0].switch = "red".into();
        map_def.pressure_plates.push(plate_def(0, 1, 0, switch));
        let err = validate_map(&map_def).expect_err("overlapping pressure plates accepted");
        assert!(err.to_string().contains("duplicates a plate"), "{err}");
        map_def.pressure_plates[1].level = 1;
        validate_map(&map_def).expect("plates on separate levels rejected");
        map_def.pressure_plates.pop();
    }
}

#[test]
fn validate_rejects_a_plate_without_a_floor_or_on_a_ramp() {
    let mut map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[0, 1]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![ramp([1, 3], [0, 1], RampDirection::East, 0)],
    );
    let plate = |level: u32, col: i32, row: i32| PressurePlateDef {
        level,
        col,
        row,
        switch: FIREWORKS.into(),
    };

    map_def.pressure_plates.push(plate(0, 0, 3));
    let err = validate_map(&map_def).expect_err("a plate on a floorless cell accepted");
    assert!(err.to_string().contains("[0, 3] has no floor"), "{err}");

    map_def.pressure_plates[0] = plate(0, 1, 0);
    let err = validate_map(&map_def).expect_err("a plate under a ramp accepted");
    assert!(err.to_string().contains("[1, 0] is inside a ramp footprint"), "{err}");

    map_def.levels[1].floors.push(floor_def(2, 0));
    map_def.pressure_plates[0] = plate(1, 2, 0);
    let err = validate_map(&map_def).expect_err("a plate on a ramp's arrival cell accepted");
    assert!(err.to_string().contains("[2, 0] is inside a ramp footprint"), "{err}");

    map_def.pressure_plates[0] = plate(0, 0, 1);
    validate_map(&map_def).expect("a plate on an inaccessible floor rejected");
    map_def.pressure_plates[0] = plate(0, 0, 0);
    validate_map(&map_def).expect("a plate on a floor rejected");
}

#[test]
fn terrain_requires_only_five_authored_faces_and_rejects_a_top_override() {
    let terrain: TerrainDef = serde_json::from_value(serde_json::json!({
        "col": 1,
        "row": 2,
        "all": "slab",
        "north": "stone"
    }))
    .expect("valid terrain rejected");
    assert_eq!(terrain.materials.top, TERRAIN_MATERIAL);
    assert_eq!(terrain.materials.bottom, "slab");
    assert_eq!(terrain.materials.north, "stone");

    let missing = serde_json::from_value::<TerrainDef>(serde_json::json!({"col": 1, "row": 2}))
        .expect_err("terrain without side materials accepted");
    assert!(missing.to_string().contains("missing terrain material"));
    let top = serde_json::from_value::<TerrainDef>(serde_json::json!({
        "col": 1,
        "row": 2,
        "all": "slab",
        "top": "grass"
    }))
    .expect_err("terrain top override accepted");
    assert!(top.to_string().contains("top"));
}

#[test]
fn terrain_lies_inside_the_grid_and_off_floors_and_ramps() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].terrain.push(cell_def(4, 0));
    let err = validate_map(&map_def).expect_err("out-of-bounds terrain accepted");
    assert!(err.to_string().contains("terrain"), "{err}");

    let mut map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        Vec::new(),
    );
    map_def.levels[0].terrain.push(cell_def(1, 0));
    let err = validate_map(&map_def).expect_err("terrain over an inaccessible floor accepted");
    assert!(err.to_string().contains("overlaps a floor"), "{err}");

    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(Vec::new())],
        Vec::new(),
        vec![ramp([1, 3], [1, 2], RampDirection::East, 0)],
    );
    map_def.levels[0].terrain.push(cell_def(0, 0));
    assert!(
        validate_map(&map_def)
            .expect_err("terrain/floor overlap accepted")
            .to_string()
            .contains("overlaps a floor")
    );
    map_def.levels[0].terrain.clear();
    map_def.levels[1].terrain.push(cell_def(1, 1));
    assert!(
        validate_map(&map_def)
            .expect_err("terrain/ramp overlap accepted")
            .to_string()
            .contains("sits on a ramp")
    );
}

#[test]
fn an_item_needs_a_known_type_and_its_own_cell_inside_the_grid() {
    for (items, expected) in [
        (vec![item_def(0, 4, 0, "gold", None)], "[4, 0] is outside the grid"),
        (vec![item_def(0, 0, 0, "banana", None)], "unknown type"),
        (
            vec![item_def(0, 0, 0, "gold", None), item_def(0, 0, 0, "speed", None)],
            "duplicates",
        ),
    ] {
        let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
        map_def.items = items;
        let err = validate_map(&map_def).expect_err("invalid item accepted");
        assert!(err.to_string().contains(expected), "{err}");
    }
}

#[test]
fn only_key_items_name_a_field_and_every_key_names_one() {
    for (item, expected) in [
        (item_def(0, 0, 0, "key", None), "unknown key field"),
        (item_def(0, 0, 0, "gold", Some("red")), "only key items"),
    ] {
        let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
        map_def.items.push(item);
        let err = validate_map(&map_def).expect_err("invalid key field accepted");
        assert!(err.to_string().contains(expected), "{err}");
    }
}

#[test]
fn ladders_on_one_edge_may_stack_but_not_overlap_from_either_side() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        Vec::new(),
    );
    map_def.ladders = vec![ladder(0, 0, 0, WallSide::East, 1), ladder(1, 0, 0, WallSide::East, 1)];
    validate_map(&map_def).expect("stacked ladders with disjoint spans rejected");
    for ladders in [
        vec![ladder(0, 0, 0, WallSide::East, 2), ladder(1, 0, 0, WallSide::East, 1)],
        // Cell (0,0)'s east edge is cell (1,0)'s west edge, and an edge holds one ladder.
        vec![ladder(0, 0, 0, WallSide::East, 1), ladder(0, 1, 0, WallSide::West, 1)],
    ] {
        map_def.ladders = ladders;
        let err = validate_map(&map_def).expect_err("overlapping ladders accepted");
        assert!(err.to_string().contains("overlaps"), "{err}");
    }
}

#[test]
fn a_ladder_spans_at_least_one_storey_inside_the_grid_and_the_levels() {
    for (def, expected) in [
        (
            ladder(0, 0, 0, WallSide::East, 2),
            "spans levels 0..2 but the map has 2 level(s)",
        ),
        (ladder(0, 0, 0, WallSide::East, 0), "at least 1"),
        (ladder(0, 5, 0, WallSide::East, 1), "outside the grid"),
    ] {
        let mut map_def = map_with_zones(
            4,
            vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
            Vec::new(),
            Vec::new(),
        );
        map_def.ladders.push(def);
        let err = format!("{:#}", validate_map(&map_def).expect_err("invalid ladder accepted"));
        assert!(err.contains(expected), "{err}");
    }
}

#[test]
fn eraser_validation_rejects_duplicates_diagonals_and_out_of_bounds_edges() {
    for edges in [
        vec![[1, 0, 1, 1], [1, 1, 1, 0]],
        vec![[1, 0, 2, 1]],
        vec![[-1, 0, 0, 0]],
    ] {
        let mut definition = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
        definition.levels[0].erasers = edges
            .into_iter()
            .map(|[c0, r0, c1, r1]| EraserDef { c0, r0, c1, r1 })
            .collect();
        assert!(validate_map(&definition).is_err());
    }
}

#[test]
fn canonicalize_keeps_zones_that_differ_only_by_switch() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![
            actor_zone(0, 0, 0),
            actor_zone(0, 0, 0),
            actor_zone(0, 0, 0),
            actor_zone(0, 0, 0),
        ],
        Vec::new(),
    );
    map_def.actor_spawn_zones[0].switch = Some("guards".into());
    map_def.actor_spawn_zones[2].switch = Some("guards".into());

    canonicalize(&mut map_def);

    assert_eq!(
        map_def
            .actor_spawn_zones
            .iter()
            .map(|zone| zone.switch.as_deref())
            .collect::<Vec<_>>(),
        [None, Some("guards")],
        "true duplicates merge; a different switch is a different zone"
    );
}

#[test]
fn actor_count_lists_validate_and_canonicalize() {
    let mut zone = actor_zone(0, 0, 0);
    let mut map = map_with_zones(4, vec![level(Vec::new())], vec![zone.clone()], vec![]);
    for counts in [vec![], vec![2, 1]] {
        map.actor_spawn_zones[0].count = counts;
        let error = validate_map(&map).expect_err("invalid count list");
        assert!(error.to_string().contains("actor_spawn_zones[0] Count"));
    }
    zone.count = vec![0, 2, 4];
    map.actor_spawn_zones = vec![zone.clone(), actor_zone(0, 0, 0), zone];
    validate_map(&map).expect("scaled count list rejected");
    canonicalize(&mut map);
    assert_eq!(map.actor_spawn_zones.len(), 2);
    assert_eq!(map.actor_spawn_zones[0].count, vec![0, 2, 4]);
    assert_eq!(map.actor_spawn_zones[1].count, vec![1]);
}
