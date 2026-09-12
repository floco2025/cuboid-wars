use super::{super::schema::CheckpointDef, *};
use common::protocol::{CheckpointKind, MapLayout};

fn checkpoint_def(level: u32, col: i32, row: i32) -> CheckpointDef {
    CheckpointDef {
        zone: ZoneDef {
            level,
            cols: [col, col + 1],
            rows: [row, row + 1],
        },
        kind: CheckpointKind::Individual,
    }
}

#[test]
fn checkpoints_require_valid_nonoverlapping_flat_floor_rectangles() {
    let mut map = map_with_zones(4, vec![level(vec![[0, 0], [1, 0]])], Vec::new(), Vec::new(), Vec::new());
    map.checkpoints.push(checkpoint_def(0, 0, 0));
    validate_map(&map).expect("checkpoint map rejected");
    let compile = |map: &MapDef| compile_with(map, &no_nested(), &empty_kind_table(), &no_bridges());
    let (layout, config) = compile(&map).expect("checkpoint compilation failed");
    assert_eq!(layout.checkpoints.len(), 1);
    let checkpoint = layout.checkpoints[0];
    assert_eq!(checkpoint.min_x, config.root_grid().geometry.cell_to_world_x(0));
    assert_eq!(checkpoint.max_x, config.root_grid().geometry.cell_to_world_x(1));
    map.checkpoints.push(checkpoint_def(0, 0, 0));
    assert!(
        validate_map(&map)
            .expect_err("overlapping checkpoints accepted")
            .to_string()
            .contains("overlaps")
    );
    map.checkpoints.pop();
    map.checkpoints[0].zone.level = 3;
    assert!(validate_map(&map).is_err());
    map.checkpoints[0].zone.level = 0;
    map.checkpoints[0].zone.cols = [0, 3];
    assert!(
        validate_map(&map)
            .expect_err("checkpoint without flat floor accepted")
            .to_string()
            .contains("flat accessible floor")
    );
    map.checkpoints[0].zone.cols = [0, 1];
    map.ramps.push(ramp([0, 0], [1, 2], 0));
    map.levels.push(level(Vec::new()));
    assert!(
        validate_map(&map)
            .expect_err("checkpoint without flat floor accepted")
            .to_string()
            .contains("flat accessible floor")
    );
}

#[test]
fn repeated_nested_checkpoints_have_separate_carriers_and_runtime_slots() {
    let mut nested = map_with_zones(2, vec![level(vec![[0, 0]])], Vec::new(), Vec::new(), Vec::new());
    nested.checkpoints.push(checkpoint_def(0, 0, 0));
    let mut root = map_with_zones(8, vec![level(Vec::new())], Vec::new(), Vec::new(), Vec::new());
    root.nested_maps = [0, 4]
        .map(|col| NestedMapDef {
            map: "platform".into(),
            motion: MotionDef {
                switch_inverted: false,

                level: 0,
                from: [col, 0],
                to: [col, 3],
                to_level: None,
                travel_secs: 2.0,
                pause_secs: 0.0,
                phase_secs: 0.0,
                from_nudge: [0.0; 3],
                to_nudge: [0.0; 3],
                switch: None,
            },
        })
        .to_vec();
    let (layout, config) = compile_with(
        &root,
        &LoadedMaps::from([("platform".into(), nested)]),
        &empty_kind_table(),
        &no_bridges(),
    )
    .expect("nested checkpoint map rejected");
    assert_eq!(layout.checkpoints.len(), 2);
    assert_ne!(layout.checkpoints[0].carrier, layout.checkpoints[1].carrier);
    assert_eq!(config.grids.len(), 3);
}

#[test]
fn checkpoint_types_are_required_and_preserved_on_the_wire() {
    for (name, kind) in [
        ("individual", CheckpointKind::Individual),
        ("group_any", CheckpointKind::GroupAny),
        ("group_all", CheckpointKind::GroupAll),
    ] {
        let definition: CheckpointDef =
            serde_json::from_value(serde_json::json!({"type": name, "level": 0, "cols": [0, 1], "rows": [0, 1]}))
                .expect("checkpoint type rejected");
        let mut map = map_with_zones(2, vec![level(vec![[0, 0]])], Vec::new(), Vec::new(), Vec::new());
        map.checkpoints.push(definition);
        let (layout, _) = compile_with(&map, &no_nested(), &empty_kind_table(), &no_bridges())
            .expect("typed checkpoint compilation failed");
        assert_eq!(layout.checkpoints[0].kind, kind);
        let bytes = bincode::encode_to_vec(&layout, bincode::config::standard()).expect("checkpoint encoding failed");
        let (decoded, _): (MapLayout, _) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("checkpoint decoding failed");
        assert_eq!(decoded.checkpoints[0].kind, kind);
    }
    for extra in [serde_json::json!({}), serde_json::json!({"type": "unknown"})] {
        let mut value = serde_json::json!({"level": 0, "cols": [0, 1], "rows": [0, 1]});
        value
            .as_object_mut()
            .expect("checkpoint object missing")
            .extend(extra.as_object().expect("extra fields missing").clone());
        assert!(serde_json::from_value::<CheckpointDef>(value).is_err());
    }
}
