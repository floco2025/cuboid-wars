use super::*;
use crate::actors::{
    TraversalEnvironment, TraversalExecutor, TraversalStatus,
    navigation::surface::SurfaceMesh,
    test_kinds::{self, CONTACT, CONTACT_BEAM},
};
use common::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_CONTACT_OFFSET, TICK_SECS},
};

fn compile_terrain_map(map: &MapDef) -> anyhow::Result<(MapLayout, MapConfig)> {
    let kinds = empty_kind_table();
    compile_with(map, &no_nested(), &kinds)
}

#[test]
fn compiled_ramps_support_actor_routes_and_movement_in_both_directions() {
    // Two cells of run per storey keep the slope the same whatever the rise.
    for levels in [1_u32, 2] {
        let far = 1 + 2 * levels as i32;
        for (cols, rows, direction, bottom_cell, top_cell) in [
            ([1, far], [1, 2], RampDirection::East, [0, 1], [far, 1]),
            ([1, far], [1, 2], RampDirection::West, [far, 1], [0, 1]),
            ([1, 2], [1, far], RampDirection::South, [1, 0], [1, far]),
            ([1, 2], [1, far], RampDirection::North, [1, far], [1, 0]),
        ] {
            let mut floors = vec![level(vec![bottom_cell])];
            floors.extend((1..levels).map(|_| level(Vec::new())));
            floors.push(level(vec![top_cell]));
            let mut ramp = ramp(cols, rows, direction, 0);
            ramp.levels = levels;
            let map_def = map_with_zones(6, floors, Vec::new(), vec![ramp]);
            let (layout, config) =
                compile_with(&map_def, &no_nested(), &empty_kind_table()).expect("ramp test map failed to compile");
            let geometry = config.root_grid().geometry;
            let rise = LEVEL_HEIGHT * levels as f32;

            // The arrival slab ends flush with the slope's high edge instead of overhanging it.
            let (min_x, max_x, min_z, max_z) = layout
                .floors
                .iter()
                .find(|floor| (floor.y - rise).abs() < 0.001)
                .expect("arrival floor missing")
                .bounds_xz();
            let (edge, expected) = match direction {
                RampDirection::East => (min_x, geometry.cell_to_world_x(far)),
                RampDirection::West => (max_x, geometry.cell_to_world_x(1)),
                RampDirection::South => (min_z, geometry.cell_to_world_z(far)),
                RampDirection::North => (max_z, geometry.cell_to_world_z(1)),
            };
            assert!((edge - expected).abs() < 0.001, "{direction:?} over {levels}");

            let carriers = Carriers::from_layout(&layout);
            let world = CollisionWorld::from_map_layout(&layout);
            let bottom = Position {
                x: geometry.cell_center_x(bottom_cell[0]),
                y: 0.0,
                z: geometry.cell_center_z(bottom_cell[1]),
            };
            let top = Position {
                x: geometry.cell_center_x(top_cell[0]),
                y: rise,
                z: geometry.cell_center_z(top_cell[1]),
            };
            for kind in [CONTACT, CONTACT_BEAM] {
                let physics = test_kinds::physics(kind);
                let navigation = SurfaceMesh::bake(
                    &world.collision_meshes().expect("collision export"),
                    CarrierId::WORLD,
                    physics,
                    &[],
                )
                .expect("ramp mesh");
                let slope = LEVEL_HEIGHT / (geometry.cell_size() * 2.0);
                // Half-way up a two-storey ramp a body stands at the height of the storey it passes.
                let middle = Position {
                    x: f32::midpoint(bottom.x, top.x),
                    y: rise / 2.0
                        + physics.movement_collider.radius() * ((1.0 + slope * slope).sqrt() - 1.0)
                        + CHARACTER_CONTACT_OFFSET * 2.0,
                    z: f32::midpoint(bottom.z, top.z),
                };
                for (start, target) in [(bottom, top), (top, bottom), (middle, top), (middle, bottom)] {
                    walk_ramp_route(&navigation, &world, &carriers, physics, start, target);
                }
            }
        }
    }
}

#[test]
fn a_plank_overhangs_its_cells_like_the_walkway_it_continues_except_along_a_wall() {
    let footprint = |shape, walled: bool| {
        let mut plank = ramp([1, 2], [1, 3], RampDirection::South, 0);
        plank.shape = shape;
        let mut map = map_with_zones(
            4,
            vec![level(vec![[1, 0]]), level(vec![[1, 3]])],
            Vec::new(),
            vec![plank],
        );
        if walled {
            map.levels[0].walls.push(WallDef {
                c0: 1,
                r0: 1,
                c1: 1,
                r1: 2,
                materials: FaceMaterials::uniform("test"),
            });
        }
        let (layout, config) =
            compile_with(&map, &no_nested(), &empty_kind_table()).expect("plank test map failed to compile");
        let geometry = config.root_grid().geometry;
        let (min_x, max_x, ..) = layout.ramps[0].bounds_xz();
        (min_x - geometry.cell_to_world_x(1), max_x - geometry.cell_to_world_x(2))
    };
    let pad = WALL_THICKNESS / 2.0;
    let close = |(west, east): (f32, f32), expected: (f32, f32)| {
        (west - expected.0).abs() < 0.001 && (east - expected.1).abs() < 0.001
    };

    assert!(close(footprint(RampShape::Plank, false), (-pad, pad)));
    assert!(close(footprint(RampShape::Plank, true), (0.0, pad)));
    assert!(close(footprint(RampShape::Solid, false), (0.0, 0.0)));
}

fn walk_ramp_route(
    mesh: &SurfaceMesh,
    world: &CollisionWorld,
    carriers: &Carriers,
    physics: CharacterPhysicsConfig,
    start: Position,
    target: Position,
) {
    let settings = map_settings();
    let env = TraversalEnvironment {
        world,
        carriers,
        settings: &settings,
        open: &[],
        delta: TICK_SECS,
    };
    let route = mesh
        .route(start, target, 1.0)
        .unwrap_or_else(|error| panic!("ramp route {start:?} -> {target:?}: {error:?}"));
    let mut executor = TraversalExecutor::new(start, physics, 3.0, &env);
    executor.set_route(route);
    for _ in 0..1200 {
        executor.step(&env);
        assert!(
            !world.character_penetrates_solid(&executor.movement.position, physics, &[]),
            "actor penetrates ramp: {executor:?}"
        );
        if executor.status == TraversalStatus::Reached {
            break;
        }
    }
    assert_eq!(
        executor.status,
        TraversalStatus::Reached,
        "actor stuck on ramp: {executor:?}"
    );
    assert!(
        executor.movement.position.distance_sq(&target) < 0.5,
        "actor missed ramp landing: {executor:?}"
    );
}

#[test]
fn inaccessible_floor_emits_physical_slab_but_not_regular_floor() {
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[2, 0]])],
        vec![actor_zone(0, 0, 0)],
        Vec::new(),
    );

    let (layout, config) = compile_with(&map_def, &no_nested(), &empty_kind_table()).expect("compile");
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
fn compile_resolves_a_barriers_field() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        field: "red".into(),
    });
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].field, common::protocol::FieldId(0));
}

#[test]
fn stacked_barriers_compile_into_one_record_when_no_floor_splits_them() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[2, 2]])],
        Vec::new(),
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            field: "red".into(),
        });
    }
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table()).expect("compile");
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
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            field: "red".into(),
        });
    }
    let (layout, _) = compile_with(&map_def, &no_nested(), &red_only_kind_table()).expect("compile");
    assert_eq!(layout.barriers.len(), 2);
    assert!(layout.barriers.iter().all(|barrier| barrier.levels == 1));
}

#[test]
fn compiled_wall_trim_blocks_portal_shots_through_the_storey_seam() {
    let mut map = map_with_zones(3, vec![level(vec![[0, 0]]), level(Vec::new())], Vec::new(), Vec::new());
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
    let (layout, config) =
        compile_with(&map, &no_nested(), &empty_kind_table()).expect("stacked wall map failed to compile");
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
        &[(
            "test".to_owned(),
            TextureSettings {
                material: "test".to_owned(),
                portalable: true,
            },
        )]
        .into(),
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
        r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": [2], "respawn_secs": 90}"#,
    )
    .expect("zone def parses");
    assert_eq!(
        (zone.respawn_secs, zone.switch, zone.beam_in_secs),
        (Some(90.0), None, 0.0)
    );
    let zone: ActorSpawnZoneDef = serde_json::from_str(
        r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": [2], "respawn_secs": 90, "beam_in_secs": 2.5}"#,
    )
    .expect("zone def with a beam-in parses");
    assert_eq!(zone.beam_in_secs, 2.5);
    let zone: ActorSpawnZoneDef = serde_json::from_str(
        r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": [2], "respawn_secs": null, "switch": "guards"}"#,
    )
    .expect("switched zone def parses");
    assert_eq!((zone.respawn_secs, zone.switch.as_deref()), (None, Some("guards")));
    assert!(
        serde_json::from_str::<ActorSpawnZoneDef>(
            r#"{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": [2]}"#
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
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0], [1, 0]])],
        vec![actor_zone(0, 1, 0)],
        Vec::new(),
    );
    map_def.pressure_plates.push(plate_def(0, 0, 0, "red"));
    map_def.actor_spawn_zones[0].switch = Some("red".into());
    let (layout, config) = compile_with(&map_def, &no_nested(), &kinds).expect("switched zone map rejected");
    let red = switch_id(&kinds, "red");
    assert_eq!(config.pressure_plates[0].switch, red);
    assert_eq!(layout.pressure_plates[0].switch, red);
    assert_eq!(config.actor_spawn_zones[0].switch, Some(red));

    map_def.actor_spawn_zones[0].switch = Some("void".into());
    let err = compile_with(&map_def, &no_nested(), &kinds).expect_err("unknown zone switch accepted");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.contains("actor_spawn_zones[0]") && chain.contains("unknown switch"),
        "{chain}"
    );

    map_def.actor_spawn_zones[0].switch = Some(FIREWORKS.into());
    let (_, config) =
        compile_with(&map_def, &no_nested(), &kinds).expect("a zone on a switch with no plate yet rejected");
    assert_eq!(config.actor_spawn_zones[0].switch, Some(switch_id(&kinds, FIREWORKS)));

    map_def.actor_spawn_zones[0].switch = None;
    map_def.pressure_plates[0].switch = "void".into();
    let err = compile_with(&map_def, &no_nested(), &kinds).expect_err("unknown plate switch accepted");
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

    let (layout, config) = compile_with(&map_def, &no_nested(), &skyway_kind_table()).expect("compile");
    let geometry = config.root_grid().geometry;

    assert_eq!(layout.light_bridges.len(), 1, "a 2x2 block is one collider");
    let bridge = layout.light_bridges[0];
    assert_eq!(bridge.field, common::protocol::FieldId(0));
    assert_eq!(bridge.level, 0);
    assert!((bridge.y - 0.0).abs() < 1e-4, "level 0 stands at y = 0");
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    let pad = geometry.wall_half_thickness();
    assert!((min_x - (geometry.cell_to_world_x(1) - pad)).abs() < 1e-4);
    assert!((max_x - (geometry.cell_to_world_x(3) + pad)).abs() < 1e-4);
    assert!((min_z - (geometry.cell_to_world_z(0) - pad)).abs() < 1e-4);
    assert!((max_z - (geometry.cell_to_world_z(2) + pad)).abs() < 1e-4);
}

#[test]
fn portal_shots_cannot_leak_through_compiled_bridge_landing_seams_or_outer_edges() {
    let mut map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);
    map_def.levels.insert(
        0,
        level((0..4).flat_map(|row| (0..4).map(move |col| [col, row])).collect()),
    );
    let (layout, config) =
        compile_with(&map_def, &no_nested(), &skyway_kind_table()).expect("bridge landing map failed to compile");
    let geometry = config.root_grid().geometry;
    let world = CollisionWorld::from_map_layout(&layout);
    let pad = geometry.wall_half_thickness();
    let floor_edge = geometry.cell_to_world_x(1) + pad;
    let bridge_edge = geometry.cell_to_world_x(3) + pad;
    let zs = [
        geometry.cell_to_world_z(0) - pad + 1e-3,
        geometry.cell_to_world_z(0) + 0.5,
        geometry.cell_to_world_z(1) + pad - 1e-3,
    ];
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
    let open: Vec<_> = layout.light_bridges.iter().map(|bridge| bridge.field).collect();
    let hit = world
        .portal_surface_along_ray(
            Vec3::new(bridge_edge - 1e-3, LEVEL_HEIGHT + 2.0, zs[0]),
            Vec3::NEG_Y,
            20.0,
            &open,
        )
        .expect("unpowered bridge blocked the lower floor");
    assert!(hit.point.y.abs() < 1e-4);
}

#[test]
fn compile_rejects_a_bridge_of_an_unknown_field() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.levels[0].light_bridges[0].field = "magenta".into();

    let err = compile_with(&map_def, &no_nested(), &skyway_kind_table()).expect_err("unknown bridge field must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(chain.contains("unknown field"), "got: {chain}");
    assert!(chain.contains("light_bridges[0]"), "got: {chain}");
}

#[test]
fn compile_rejects_unknown_barrier_kind() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        field: "magenta".into(),
    });
    let err = compile_with(&map_def, &no_nested(), &red_only_kind_table()).expect_err("unknown kind must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.to_lowercase().contains("magenta") || chain.to_lowercase().contains("unknown field"),
        "expected 'magenta' or 'unknown field' somewhere in chain; got: {chain}"
    );
}

#[test]
fn compile_resolves_three_distinct_kinds() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        field: "red".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 2,
        r1: 0,
        field: "blue".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 2,
        r0: 0,
        c1: 3,
        r1: 0,
        field: "green".into(),
    });
    let (layout, _) = compile_with(&map_def, &no_nested(), &three_kind_table()).expect("compile");
    assert_eq!(layout.barriers.len(), 3);
    let fields: Vec<u16> = layout.barriers.iter().map(|b| b.field.0).collect();
    // The merger sorts by (level, field, axis-coords), so field ascending.
    assert_eq!(fields, vec![0, 1, 2]);
}

#[test]
fn terrain_creates_accessible_floor_slabs_without_supporting_floors() {
    let mut map_def = map_with_zones(4, vec![level(vec![[3, 3]])], Vec::new(), Vec::new());
    map_def.levels[0].terrain.push(cell_def(0, 0));
    map_def.levels[0].terrain.push(cell_def(2, 2));
    let (layout, config) = compile_terrain_map(&map_def).expect("compile");
    assert_eq!(layout.terrain.len(), 2);
    assert_eq!(layout.terrain[0].level, 0);
    assert!(config.root_grid().levels[0].cells.rows[0][0].has_floor);
    assert!(config.root_grid().levels[0].cells.rows[2][2].has_floor_slab);
    assert!(
        layout
            .floor_materials
            .iter()
            .any(|materials| materials.top == TERRAIN_MATERIAL)
    );
}

#[test]
fn terrain_does_not_require_exterior_grounds() {
    let mut map_def = map_with_zones(4, vec![level(vec![[3, 3]])], Vec::new(), Vec::new());
    map_def.levels[0].terrain.push(cell_def(0, 0));
    let (layout, _) =
        compile_with(&map_def, &no_nested(), &empty_kind_table()).expect("standalone terrain failed to compile");
    assert_eq!(layout.terrain.len(), 1);
}

#[test]
fn terrain_compiles_to_cell_center_and_floor_top() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]]), level(Vec::new())], Vec::new(), Vec::new());
    map_def.levels[1].terrain.push(cell_def(1, 2));
    let (layout, config) = compile_terrain_map(&map_def).expect("compile");
    let geometry = config.root_grid().geometry;
    assert_eq!(layout.terrain.len(), 1);
    let cell = layout.terrain[0];
    assert_eq!(cell.level, 1);
    let expected_x = geometry.cell_center_x(1);
    let expected_z = geometry.cell_center_z(2);
    assert!((cell.x - expected_x).abs() < 1e-5);
    assert!((cell.z - expected_z).abs() < 1e-5);
    assert!((cell.y - LEVEL_HEIGHT).abs() < 1e-5);
}

#[test]
fn terrain_may_not_duplicate_an_inaccessible_floor() {
    let mut map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        Vec::new(),
    );
    map_def.levels[0].terrain.push(cell_def(1, 0));
    let error = validate_map(&map_def).expect_err("terrain must be its own slab");
    assert!(error.to_string().contains("overlaps a floor"));
}

#[test]
fn compile_rejects_item_on_floorless_cell() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.items.push(item_def(0, 2, 2, "gold", None));
    let err =
        compile_with(&map_def, &no_nested(), &empty_kind_table()).expect_err("item on a floorless cell must fail");
    assert!(err.to_string().contains("floor"));
}

#[test]
fn compile_rejects_item_on_ramp_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        Vec::new(),
        vec![ramp([0, 1], [0, 2], RampDirection::South, 1)],
    );
    map_def.items.push(item_def(1, 0, 0, "gold", None));
    let err = compile_with(&map_def, &no_nested(), &empty_kind_table()).expect_err("item on a ramp cell must fail");
    assert!(err.to_string().contains("ramp"));
}

#[test]
fn compile_resolves_key_item_barrier_kind() {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.items.push(item_def(0, 0, 0, "key", Some("red")));
    let (_, config) = compile_with(&map_def, &no_nested(), &red_only_kind_table()).expect("compile");
    assert_eq!(config.placed_items.len(), 1);
    assert_eq!(
        config.placed_items[0].item_type,
        common::protocol::ItemType::Key(common::protocol::FieldId(0))
    );
}

#[test]
fn ladder_compiles_to_world_segment_and_normal() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[1, 1]]), level(vec![[1, 0]])],
        Vec::new(),
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 1, 1, WallSide::North, 1));

    let (layout, _) = compile_with(&map_def, &no_nested(), &empty_kind_table()).expect("compile");

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
    let mut definition = map_with_zones(4, vec![level(vec![[0, 0], [1, 0]])], Vec::new(), Vec::new());
    definition.levels[0].erasers.push(EraserDef {
        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
    });
    validate_map(&definition).expect("eraser map rejected");
    let (layout, config) =
        compile_with(&definition, &no_nested(), &empty_kind_table()).expect("eraser map failed to compile");
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
fn null_nested_destination_inherits_the_starting_level() {
    let definition: MotionDef = serde_json::from_value(serde_json::json!({
        "level": 2, "from": [0, 0], "to": [1, 0], "to_level": null, "travel_secs": 2.0
    }))
    .expect("null destination rejected");
    assert_eq!(definition.to_level(), 2);
}
