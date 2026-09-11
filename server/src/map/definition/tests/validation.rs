use super::*;

#[test]
fn validation_accepts_actor_zone_without_floor() {
    // Empty cells (no floor at all) are allowed: kinds like flying actors
    // don't need a floor underfoot. Forbidden cells are obstructions.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[1, 0]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("a zone over an empty cell should load");
}

#[test]
fn validation_accepts_actor_zone_on_higher_level_floor() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[1, 0]]), level(vec![[0, 0]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 1, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("zones should be allowed on any level floor");
}

#[test]
fn validation_accepts_actor_zone_overlapping_inaccessible_floor() {
    // Spawn zones may freely cover any cell, including inaccessible-floor slabs.
    // The runtime spawn picker filters non-spawnable cells out at pick time
    // (see `Cell::is_spawnable`), so authoring a zone that brushes one is fine.
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        vec![actor_zone(0, 1, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("actor zone overlapping inaccessible floor should load");
}

#[test]
fn validation_accepts_player_zone_overlapping_inaccessible_floor() {
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 1, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("player zone overlapping inaccessible floor should load");
}

#[test]
fn validation_accepts_actor_zone_overlapping_ramp_footprint() {
    // Ramp footprints are not spawnable, but a zone is free to brush one;
    // the spawn picker skips ramp cells (see `Cell::is_spawnable`).
    let map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 3, 3)],
        vec![ramp([0, 0], [1, 2], 1)],
    );

    validate_map(&map_def).expect("actor zone overlapping ramp footprint should load");
}

#[test]
fn validation_accepts_player_zone_overlapping_ramp_footprint() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        vec![],
        vec![player_zone(1, 0, 0)],
        vec![ramp([0, 0], [1, 2], 1)],
    );

    validate_map(&map_def).expect("player zone overlapping ramp footprint should load");
}

#[test]
fn validation_rejects_actor_zone_with_empty_kind() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![ActorSpawnZoneDef {
            switch_inverted: false,

            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: String::new(),
            count: 1,
            respawn_secs: Some(90.0),
            switch: None,
        }],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    let err = validate_map(&map_def).expect_err("must reject empty `kind`");
    assert!(err.to_string().contains("empty `kind`"));
}

#[test]
fn validation_accepts_unknown_kind_strings() {
    // The map loader knows nothing about specific kinds; whether a kind is
    // useful is the spawn picker's call. So unfamiliar strings must load fine.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![ActorSpawnZoneDef {
            switch_inverted: false,

            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: "boss".into(),
            count: 1,
            respawn_secs: Some(90.0),
            switch: None,
        }],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("any non-empty kind string should load");
}

#[test]
fn validation_accepts_empty_actor_spawn_zones() {
    // A map with no enemies is valid.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("empty actor zones should be allowed");
}

#[test]
fn validation_accepts_barrier_on_empty_edge() {
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

        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    validate_map(&map_def).expect("barrier on an empty grid edge should load");
}

#[test]
fn validation_rejects_barrier_overlapping_wall() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].walls.push(WallDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        materials: FaceMaterials::uniform("test"),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 1,
        r0: 0,
        c1: 0,
        r1: 0,
        kind: "blue".into(),
    });
    let err = validate_map(&map_def).expect_err("barrier on a wall edge must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("overlaps a wall"), "got: {msg}");
}

#[test]
fn validate_rejects_a_light_bridge_on_a_floor() {
    let map_def = map_with_bridges(&[[0, 0]]);

    let err = validate_map(&map_def).expect_err("a bridge on a floor must fail");
    assert!(err.to_string().contains("sits on a floor"), "got: {err}");
}

#[test]
fn validate_rejects_a_light_bridge_on_a_ramp() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        vec![ramp([1, 0], [3, 1], 0)],
    );
    map_def.levels[0].light_bridges.push(bridge_def(1, 0));

    let err = validate_map(&map_def).expect_err("a bridge on a ramp must fail");
    assert!(err.to_string().contains("sits on a ramp"), "got: {err}");
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
        vec![player_zone(0, 0, 0)],
        vec![ramp([1, 0], [3, 1], 0)],
    );
    let plate = |level: u32, col: i32, row: i32| PressurePlateDef {
        level,
        col,
        row,
        switch: FIREWORKS.into(),
    };

    map_def.pressure_plates.push(plate(0, 0, 3));
    let err = validate_map(&map_def).expect_err("a plate on a floorless cell accepted");
    assert!(err.to_string().contains("has no floor at level 0 col 0 row 3"), "{err}");

    map_def.pressure_plates[0] = plate(0, 1, 0);
    let err = validate_map(&map_def).expect_err("a plate under a ramp accepted");
    assert!(
        err.to_string().contains("sits on a ramp at level 0 col 1 row 0"),
        "{err}"
    );

    map_def.levels[1].floors.push(floor_def(2, 0));
    map_def.pressure_plates[0] = plate(1, 2, 0);
    let err = validate_map(&map_def).expect_err("a plate on a ramp's arrival cell accepted");
    assert!(
        err.to_string().contains("sits on a ramp at level 1 col 2 row 0"),
        "{err}"
    );

    map_def.pressure_plates[0] = plate(0, 0, 1);
    validate_map(&map_def).expect("a plate on an inaccessible floor rejected");
    map_def.pressure_plates[0] = plate(0, 0, 0);
    validate_map(&map_def).expect("a plate on a floor rejected");
}

#[test]
fn validate_rejects_duplicate_light_bridge_cells() {
    let map_def = map_with_bridges(&[[1, 0], [1, 0]]);

    let err = validate_map(&map_def).expect_err("duplicate bridge cells must fail");
    assert!(err.to_string().contains("duplicate light_bridge"), "got: {err}");
}

#[test]
fn validation_rejects_grass_out_of_bounds() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(4, 0));
    let err = validate_map(&map_def).expect_err("out-of-bounds grass must be rejected");
    assert!(err.to_string().contains("grass"));
}

#[test]
fn validation_rejects_item_outside_grid() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 4, 0, "gold", None));
    let err = validate_map(&map_def).expect_err("out-of-bounds item must be rejected");
    assert!(err.to_string().contains("col"));
}

#[test]
fn validation_rejects_key_item_without_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "key", None));
    let err = validate_map(&map_def).expect_err("key without kind must be rejected");
    assert!(err.to_string().contains("kind"));
}

#[test]
fn validation_rejects_kind_on_non_key_item() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "gold", Some("red")));
    let err = validate_map(&map_def).expect_err("kind on non-key item must be rejected");
    assert!(err.to_string().contains("only key items"));
}

#[test]
fn validation_rejects_unknown_item_type() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "banana", None));
    let err = validate_map(&map_def).expect_err("unknown item type must be rejected");
    assert!(err.to_string().contains("unknown item type"));
}

#[test]
fn validation_rejects_duplicate_item_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "gold", None));
    map_def.items.push(item_def(0, 0, 0, "speed", None));
    let err = validate_map(&map_def).expect_err("two items on one cell must be rejected");
    assert!(err.to_string().contains("duplicates"));
}

#[test]
fn validation_rejects_duplicate_barrier() {
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

        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 1,
        r0: 0,
        c1: 0,
        r1: 0,
        kind: "green".into(),
    });
    let err = validate_map(&map_def).expect_err("duplicate barrier edge must be rejected");
    assert!(err.to_string().contains("duplicate barrier"));
}

#[test]
fn validation_accepts_stacked_non_overlapping_ladders() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 1));
    map_def.ladders.push(ladder(1, 0, 0, WallSide::East, 1));

    validate_map(&map_def).expect("stacked ladders with disjoint spans should validate");
}

#[test]
fn validation_rejects_ladder_span_past_top_level() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 2));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("span past the top level must be rejected")
    );
    assert!(err.contains("does not exist"));
}

#[test]
fn validation_rejects_zero_storey_ladder() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 0));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("zero-storey ladder must be rejected")
    );
    assert!(err.contains("at least 1"));
}

#[test]
fn validation_rejects_overlapping_ladders_on_same_edge() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 2));
    map_def.ladders.push(ladder(1, 0, 0, WallSide::East, 1));

    let err = validate_map(&map_def).expect_err("overlapping ladder spans must be rejected");
    assert!(err.to_string().contains("overlaps"));
}

#[test]
fn validation_rejects_mirrored_ladders_on_same_edge() {
    // Cell (0,0)'s east edge is cell (1,0)'s west edge; an edge holds at
    // most one ladder, so the mirrored pair is rejected as a duplicate.
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 1));
    map_def.ladders.push(ladder(0, 1, 0, WallSide::West, 1));

    let err = validate_map(&map_def).expect_err("mirrored ladders on one edge must be rejected");
    assert!(err.to_string().contains("overlaps"));
}

#[test]
fn validation_rejects_out_of_bounds_ladder() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 5, 0, WallSide::East, 1));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("out-of-bounds ladder must be rejected")
    );
    assert!(err.contains("out of grid bounds"));
}

#[test]
fn eraser_validation_rejects_duplicates_diagonals_and_out_of_bounds_edges() {
    for edges in [
        vec![[1, 0, 1, 1], [1, 1, 1, 0]],
        vec![[1, 0, 2, 1]],
        vec![[-1, 0, 0, 0]],
    ] {
        let mut definition = map_with_zones(
            4,
            vec![level(vec![[0, 0]])],
            Vec::new(),
            vec![player_zone(0, 0, 0)],
            Vec::new(),
        );
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
        vec![player_zone(0, 0, 0)],
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
fn validate_rejects_empty_switch_names() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![actor_zone(0, 0, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.pressure_plates.push(plate_def(0, 0, 0, ""));
    let err = validate_map(&map_def).expect_err("a plate with an empty switch accepted");
    assert!(
        err.to_string().contains("pressure_plates[0] has empty `switch`"),
        "{err}"
    );
    map_def.pressure_plates.clear();

    map_def.actor_spawn_zones[0].switch = Some(String::new());
    let err = validate_map(&map_def).expect_err("a zone with an empty switch accepted");
    assert!(
        err.to_string().contains("actor_spawn_zones[0] has empty `switch`"),
        "{err}"
    );
}
