use super::*;

#[test]
fn inaccessible_floor_emits_physical_slab_but_not_regular_floor() {
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[2, 0]])],
        vec![actor_zone(0, 0, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    let (layout, config) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    let geometry = config.root_grid().geometry;
    let inaccessible_cell = config.root_grid().levels[0].cells.rows[0][2];
    assert!(!inaccessible_cell.has_floor);
    assert!(inaccessible_cell.has_floor_slab);

    let x = geometry.cell_center_x(2);
    let z = geometry.cell_center_z(0);
    assert!(layout.floors.iter().any(|floor| {
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        min_x <= x && x <= max_x && min_z <= z && z <= max_z
    }));
}

#[test]
fn compile_resolves_known_barrier_kind() {
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
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].kind, common::protocol::BarrierKindId(0));
}

#[test]
fn stacked_barriers_compile_into_one_record_when_no_floor_splits_them() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[2, 2]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            switch: None,
            switch_inverted: false,

            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            kind: "red".into(),
        });
    }
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].level, 0);
    assert_eq!(layout.barriers[0].levels, 2);
    assert_eq!(layout.barriers[0].height, LEVEL_HEIGHT + WALL_HEIGHT);
}

#[test]
fn a_floor_beside_the_upper_barrier_keeps_the_storeys_apart() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            switch: None,
            switch_inverted: false,

            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            kind: "red".into(),
        });
    }
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 2);
    assert!(layout.barriers.iter().all(|barrier| barrier.levels == 1));
}

#[test]
fn pressure_plate_barrier_is_open_for_pathfinding() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    // Vertical edge between cols 0 and 1 → `vertical[0][1]`; kind "red" has a plate.
    map_def.levels[0].barriers.push(BarrierDef {
        switch: Some("red".into()),
        switch_inverted: false,

        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    // Vertical edge between cols 1 and 2 → `vertical[0][2]`; kind "blue" has none.
    map_def.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 2,
        r0: 0,
        c1: 2,
        r1: 1,
        kind: "blue".into(),
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        switch: "red".into(),
    });

    let (_, config) = compile_with(&map_def, &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    let barrier_edges = &config.root_grid().levels[0].barrier_edges;
    assert!(
        !barrier_edges.vertical[0][1],
        "pressure-plate (red) barrier must be treated as open for nav"
    );
    assert!(barrier_edges.vertical[0][2], "non-plate (blue) barrier must block nav");
}

#[test]
fn compiled_wall_trim_blocks_portal_shots_through_the_storey_seam() {
    let mut map = map_with_zones(
        3,
        vec![level(vec![[0, 0]]), level(Vec::new())],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map.levels {
        for col in [1, 3] {
            level.walls.push(WallDef {
                c0: col,
                r0: 0,
                c1: col,
                r1: 1,
                materials: FaceMaterials::uniform("test"),
            });
        }
    }
    let (layout, config) = compile_with(&map, &no_nested(), &empty_kind_table(), &no_bridges())
        .expect("stacked wall map failed to compile");
    let world = CollisionWorld::from_map_layout(&layout);
    let geometry = config.root_grid().geometry;
    let seam_y = geometry.level_y(1) - geometry.floor_thickness() / 2.0;
    let placement = compute_portal_placement(
        Vec3::new(geometry.cell_center_x(0), seam_y, geometry.cell_center_z(0)),
        Vec3::X,
        0.0,
        geometry.width(),
        &world,
        &layout,
        &Carriers::default(),
        &[],
        &[("test".to_owned(), TextureSettings { portalable: true })].into(),
    )
    .expect("stacked wall seam has no fitting portal surface");
    let front_face = geometry.cell_to_world_x(1) - geometry.wall_half_thickness();
    assert!((placement.pos.x - front_face).abs() < 1e-4);
    assert!(placement.normal.abs_diff_eq(Vec3::NEG_X, 1e-4));
}

#[test]
fn plate_zone_and_motion_defs_parse_their_switch() {
    let json = r#"[
        {"level": 1, "col": 2, "row": 3, "switch": "red"},
        {"level": 0, "col": 4, "row": 5, "switch": "fireworks"}
    ]"#;
    let defs: Vec<PressurePlateDef> = serde_json::from_str(json).expect("plate defs parse");
    assert_eq!(defs[0].switch, "red");
    assert_eq!(
        (defs[1].level, defs[1].col, defs[1].row, defs[1].switch.as_str()),
        (0, 4, 5, "fireworks")
    );
    assert!(serde_json::from_str::<PressurePlateDef>(r#"{"level": 1, "col": 2, "row": 3}"#).is_err());
    assert!(
        serde_json::from_str::<PressurePlateDef>(r#"{"level": 1, "col": 2, "row": 3, "type": "firework"}"#).is_err()
    );

    let zone: ActorSpawnZoneDef = serde_json::from_str(
        r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 2, "respawn_secs": 90}"#,
    )
    .expect("zone def parses");
    assert_eq!((zone.respawn_secs, zone.switch), (Some(90.0), None));
    let zone: ActorSpawnZoneDef = serde_json::from_str(
        r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 2, "respawn_secs": null, "switch": "guards"}"#,
    )
    .expect("switched zone def parses");
    assert_eq!((zone.respawn_secs, zone.switch.as_deref()), (None, Some("guards")));
    assert!(
        serde_json::from_str::<ActorSpawnZoneDef>(
            r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 2}"#
        )
        .is_err(),
        "respawn_secs must be explicit"
    );

    let motion: NestedMapDef =
        serde_json::from_str(r#"{"map": "lift", "level": 0, "from": [0, 0], "to": [0, 3], "travel_secs": 2.0}"#)
            .expect("nested map def parses");
    assert_eq!(motion.motion.switch, None);
    let motion: NestedMapDef = serde_json::from_str(
        r#"{"map": "lift", "level": 0, "from": [0, 0], "to": [0, 3], "travel_secs": 2.0, "switch": "lift"}"#,
    )
    .expect("switched nested map def parses");
    assert_eq!(motion.motion.switch.as_deref(), Some("lift"));
}

#[test]
fn compile_resolves_switches_and_rejects_unknown_or_unplated_ones() {
    let kinds = red_only_kind_table();
    let bridges = no_bridges();
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0], [1, 0]])],
        vec![actor_zone(0, 1, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.pressure_plates.push(plate_def(0, 0, 0, "red"));
    map_def.actor_spawn_zones[0].switch = Some("red".into());
    let (layout, config) = compile_with(&map_def, &no_nested(), &kinds, &bridges).expect("switched zone map rejected");
    let red = switch_id(&kinds, &bridges, "red");
    assert_eq!(config.pressure_plates[0].switch, red);
    assert_eq!(layout.pressure_plates[0].switch, red);
    assert_eq!(config.actor_spawn_zones[0].switch, Some(red));

    map_def.actor_spawn_zones[0].switch = Some("void".into());
    let err = compile_with(&map_def, &no_nested(), &kinds, &bridges).expect_err("unknown zone switch accepted");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.contains("actor_spawn_zones[0]") && chain.contains("unknown switch"),
        "{chain}"
    );

    map_def.actor_spawn_zones[0].switch = Some(FIREWORKS.into());
    let err = compile_with(&map_def, &no_nested(), &kinds, &bridges).expect_err("unplated zone switch accepted");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(chain.contains("operated by no pressure plate"), "{chain}");

    map_def.actor_spawn_zones[0].switch = None;
    map_def.pressure_plates[0].switch = "void".into();
    let err = compile_with(&map_def, &no_nested(), &kinds, &bridges).expect_err("unknown plate switch accepted");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.contains("pressure_plates[0]") && chain.contains("unknown switch"),
        "{chain}"
    );
}

#[test]
fn compile_merges_light_bridge_cells_into_one_rectangle() {
    let mut map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);
    map_def.levels[0].floors = vec![floor_def(0, 3)];
    map_def.player_spawn_zones = vec![player_zone(0, 0, 3)];

    let (layout, config) =
        compile_with(&map_def, &no_nested(), &empty_kind_table(), &skyway_bridge_table()).expect("compile");
    let geometry = config.root_grid().geometry;

    assert_eq!(layout.light_bridges.len(), 1, "a 2x2 block is one collider");
    let bridge = layout.light_bridges[0];
    assert_eq!(bridge.kind, common::protocol::BridgeKindId(0));
    assert_eq!(bridge.level, 0);
    assert!((bridge.y - 0.0).abs() < 1e-4, "level 0 stands at y = 0");
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    let pad = geometry.wall_half_thickness();
    assert!((min_x - (geometry.cell_to_world_x(1) - pad)).abs() < 1e-4);
    assert!((max_x - (geometry.cell_to_world_x(3) + pad)).abs() < 1e-4);
    assert!((min_z - (geometry.cell_to_world_z(0) - pad)).abs() < 1e-4);
    assert!((max_z - (geometry.cell_to_world_z(2) + pad)).abs() < 1e-4);
    let cells = &config.root_grid().levels[0].cells.rows;
    for (row, cells) in cells.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let covered = (1..3).contains(&col) && (0..2).contains(&row);
            assert_eq!(cell.bridge, covered.then_some(bridge.id), "cell ({col}, {row})");
        }
    }
}

#[test]
fn portal_shots_cannot_leak_through_compiled_bridge_landing_seams_or_outer_edges() {
    let mut map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);
    map_def.levels.insert(
        0,
        level((0..4).flat_map(|row| (0..4).map(move |col| [col, row])).collect()),
    );
    let (layout, config) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &skyway_bridge_table())
        .expect("bridge landing map failed to compile");
    let geometry = config.root_grid().geometry;
    let mut world = CollisionWorld::from_map_layout(&layout);
    let pad = geometry.wall_half_thickness();
    let floor_edge = geometry.cell_to_world_x(1) + pad;
    let bridge_edge = geometry.cell_to_world_x(3) + pad;
    let zs = [
        geometry.cell_to_world_z(0) - pad + 1e-3,
        geometry.cell_to_world_z(0) + 0.5,
        geometry.cell_to_world_z(1) + pad - 1e-3,
    ];
    world.set_powered_bridges(&layout.light_bridges.iter().map(|bridge| bridge.id).collect::<Vec<_>>());
    for z in zs {
        for x in [floor_edge - 1e-3, floor_edge, floor_edge + 1e-3, bridge_edge - 1e-3] {
            let origin = Vec3::new(x, LEVEL_HEIGHT + 2.0, z);
            if let Some(hit) = world.portal_surface_along_ray(origin, Vec3::NEG_Y, 20.0, &[]) {
                assert!(
                    (hit.point.y - LEVEL_HEIGHT).abs() < 1e-4,
                    "shot leaked to the lower floor at ({x}, {z})"
                );
            }
        }
    }
    world.set_powered_bridges(&[]);
    let hit = world
        .portal_surface_along_ray(
            Vec3::new(bridge_edge - 1e-3, LEVEL_HEIGHT + 2.0, zs[0]),
            Vec3::NEG_Y,
            20.0,
            &[],
        )
        .expect("unpowered bridge blocked the lower floor");
    assert!(hit.point.y.abs() < 1e-4);
}

#[test]
fn compile_rejects_unknown_bridge_kind() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.levels[0].light_bridges[0].kind = "magenta".into();

    let err = compile_with(&map_def, &no_nested(), &empty_kind_table(), &skyway_bridge_table())
        .expect_err("unknown bridge kind must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(chain.contains("unknown bridge kind"), "got: {chain}");
    assert!(chain.contains("light_bridges[0]"), "got: {chain}");
}

#[test]
fn compile_rejects_unknown_barrier_kind() {
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
        kind: "magenta".into(),
    });
    let err = compile_with(&map_def, &no_nested(), &red_only_kind_table(), &no_bridges())
        .expect_err("unknown kind must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.to_lowercase().contains("magenta") || chain.to_lowercase().contains("unknown barrier kind"),
        "expected 'magenta' or 'unknown barrier kind' somewhere in chain; got: {chain}"
    );
}

#[test]
fn compile_resolves_three_distinct_kinds() {
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
        c1: 2,
        r1: 0,
        kind: "blue".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        switch: None,
        switch_inverted: false,

        c0: 2,
        r0: 0,
        c1: 3,
        r1: 0,
        kind: "green".into(),
    });
    let (layout, _) = compile_with(&map_def, &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 3);
    let kinds: Vec<u16> = layout.barriers.iter().map(|b| b.kind.0).collect();
    // The merger sorts by (level, kind, axis-coords), so kind ascending.
    assert_eq!(kinds, vec![0, 1, 2]);
}

#[test]
fn compile_drops_grass_without_floor() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(0, 0));
    map_def.levels[0].grass.push(cell_def(2, 2));
    let (layout, _) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.grass.len(), 1);
    assert_eq!(layout.grass[0].level, 0);
}

#[test]
fn grass_compiles_to_cell_center_and_floor_top() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[1, 2]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[1].grass.push(cell_def(1, 2));
    let (layout, config) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    let geometry = config.root_grid().geometry;
    assert_eq!(layout.grass.len(), 1);
    let cell = layout.grass[0];
    assert_eq!(cell.level, 1);
    let expected_x = geometry.cell_center_x(1);
    let expected_z = geometry.cell_center_z(2);
    assert!((cell.x - expected_x).abs() < 1e-5);
    assert!((cell.z - expected_z).abs() < 1e-5);
    assert!((cell.y - LEVEL_HEIGHT).abs() < 1e-5);
}

#[test]
fn grass_allowed_on_inaccessible_floor() {
    let mut map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(1, 0));
    validate_map(&map_def).expect("grass on an inaccessible floor should load");
    let (layout, _) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.grass.len(), 1);
}

#[test]
fn compile_rejects_item_on_floorless_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 2, 2, "gold", None));
    let err = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges())
        .expect_err("item on a floorless cell must fail");
    assert!(err.to_string().contains("floor"));
}

#[test]
fn compile_rejects_item_on_ramp_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        Vec::new(),
        vec![player_zone(0, 3, 3)],
        vec![ramp([0, 0], [1, 2], 1)],
    );
    map_def.items.push(item_def(1, 0, 0, "gold", None));
    let err = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges())
        .expect_err("item on a ramp cell must fail");
    assert!(err.to_string().contains("ramp"));
}

#[test]
fn compile_resolves_key_item_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "key", Some("red")));
    let (_, config) = compile_with(&map_def, &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(config.placed_items.len(), 1);
    assert_eq!(
        config.placed_items[0].item_type,
        common::protocol::ItemType::Key(common::protocol::BarrierKindId(0))
    );
}

#[test]
fn ladder_compiles_to_world_segment_and_normal() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[1, 1]]), level(vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 1, 1)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 1, 1, WallSide::North, 1));

    let (layout, _) = compile_with(&map_def, &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");

    assert_eq!(layout.ladders.len(), 1);
    let out = layout.ladders[0];
    // 4x4 grid centered on the origin: cell (1,1) spans world -3.4..0.0 on
    // both axes; its north edge lies at z = -3.4 with the cell center at
    // x = -1.7. The segment is LADDER_WIDTH wide around that midpoint.
    let half_width = common::constants::LADDER_WIDTH / 2.0;
    assert!((out.x1 - (-1.7 - half_width)).abs() < 1e-5);
    assert!((out.x2 - (-1.7 + half_width)).abs() < 1e-5);
    assert!((out.z1 - -3.4).abs() < 1e-5);
    assert!((out.z2 - -3.4).abs() < 1e-5);
    assert_eq!((out.nx, out.nz), (0.0, -1.0));
    assert_eq!((out.level, out.levels), (0, 1));
}

#[test]
fn eraser_edges_compile_to_full_storey_volumes_without_solid_geometry() {
    let mut definition = map_with_zones(
        4,
        vec![level(vec![[0, 0], [1, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    definition.levels[0].erasers.push(EraserDef {
        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
    });
    validate_map(&definition).expect("eraser map rejected");
    let (layout, config) = compile_with(&definition, &no_nested(), &empty_kind_table(), &no_bridges())
        .expect("eraser map failed to compile");
    assert_eq!(layout.erasers.len(), 1);
    let field = layout.erasers[0];
    assert_eq!(field.height, LEVEL_HEIGHT);
    assert_eq!(field.carrier, CarrierId::WORLD);
    let geometry = config.root_grid().geometry;
    assert_eq!(field.x1, geometry.cell_to_world_x(1));
    assert_eq!(field.z2, geometry.cell_to_world_z(1));
    assert!(layout.barriers.is_empty());
    assert!(layout.walls.is_empty());
}

#[test]
fn same_appearance_targets_keep_independent_controls_and_instance_ids() {
    let mut map = map_with_zones(
        4,
        vec![level(vec![[0, 0], [3, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for (col, switch) in [(0, "red"), (3, "blue")] {
        map.pressure_plates.push(PressurePlateDef {
            level: 0,
            col,
            row: 0,
            switch: switch.into(),
        });
    }
    for (col, switch) in [(0, Some("red")), (1, Some("blue")), (2, None)] {
        map.levels[0].barriers.push(BarrierDef {
            c0: col,
            r0: 1,
            c1: col + 1,
            r1: 1,
            kind: "red".into(),
            switch: switch.map(str::to_owned),
            switch_inverted: switch == Some("blue"),
        });
    }
    for (col, switch) in [(1, "red"), (2, "blue")] {
        map.levels[0].light_bridges.push(LightBridgeDef {
            col,
            row: 2,
            kind: "skyway".into(),
            switch: Some(switch.into()),
            switch_inverted: switch == "blue",
        });
    }
    let (layout, _) = compile_with(&map, &no_nested(), &three_kind_table(), &skyway_bridge_table())
        .expect("independent targets rejected");
    assert_eq!(layout.barriers.len(), 3);
    assert!(layout.barriers.windows(2).all(|pair| pair[0].id != pair[1].id));
    assert!(
        layout
            .barriers
            .iter()
            .all(|barrier| barrier.kind == layout.barriers[0].kind)
    );
    assert!(layout.barriers.iter().any(|barrier| barrier.switch.is_none()));
    assert_eq!(
        layout.barriers.iter().filter(|barrier| barrier.switch_inverted).count(),
        1
    );
    let bridges = &layout.light_bridges;
    assert!(bridges.iter().any(|bridge| !bridge.switch_inverted));
    assert!(bridges.iter().any(|bridge| bridge.switch_inverted));
    for (index, bridge) in bridges.iter().enumerate() {
        assert_eq!(bridge.kind, bridges[0].kind);
        let name = if bridge.switch_inverted { "blue" } else { "red" };
        assert_eq!(
            bridge.switch,
            Some(switch_id(&three_kind_table(), &skyway_bridge_table(), name))
        );
        assert!(bridges[index + 1..].iter().all(|other| bridge.id != other.id));
    }
}
