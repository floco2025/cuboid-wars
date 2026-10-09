use super::*;
use common::protocol::{CheckpointKind, RampDirection};
use map_core::schema::CheckpointDef;

fn checkpoint_def(level: u32, col: i32, row: i32) -> CheckpointDef {
    CheckpointDef {
        zone: ZoneDef {
            level,
            cols: [col, col + 1],
            rows: [row, row + 1],
        },
        kind: CheckpointKind::Individual,
        number: 1,
    }
}

#[test]
fn checkpoints_require_valid_nonoverlapping_flat_floor_rectangles() {
    let mut map = map_with_zones(4, vec![level(vec![[0, 0], [1, 0]])], Vec::new(), Vec::new());
    map.checkpoints.push(checkpoint_def(0, 0, 0));
    validate_map(&map).expect("checkpoint map rejected");
    let (layout, config) = compile_bare(&map).expect("checkpoint compilation failed");
    assert_eq!(layout.checkpoints.len(), 1);
    let checkpoint = &layout.checkpoints[0];
    assert_eq!(checkpoint.min_x, config.root_grid().geometry.cell_to_world_x(0));
    assert_eq!(checkpoint.max_x, config.root_grid().geometry.cell_to_world_x(1));
    map.checkpoints.push(CheckpointDef {
        number: 2,
        ..checkpoint_def(0, 0, 0)
    });
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
    map.ramps.push(ramp([0, 1], [0, 2], RampDirection::South, 0));
    map.levels.push(level(Vec::new()));
    assert!(
        validate_map(&map)
            .expect_err("checkpoint without flat floor accepted")
            .to_string()
            .contains("flat accessible floor")
    );
}

#[test]
fn terrain_is_an_accessible_checkpoint_floor() {
    let mut map = map_with_zones(4, vec![level(Vec::new())], Vec::new(), Vec::new());
    map.levels[0].terrain.push(cell_def(1, 1));
    map.checkpoints.push(checkpoint_def(0, 1, 1));
    validate_map(&map).expect("checkpoint on terrain rejected");
}

#[test]
fn repeated_nested_checkpoints_have_separate_carriers_and_runtime_slots() {
    let mut nested = map_with_zones(2, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    nested.checkpoints.push(checkpoint_def(0, 0, 0));
    let mut root = map_with_zones(8, vec![level(Vec::new())], Vec::new(), Vec::new());
    root.nested_maps = [0, 4]
        .map(|col| NestedMapDef {
            map: "platform".into(),
            motion: MotionDef {
                motion: Default::default(),
                initially_on: true,

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
    )
    .expect("nested checkpoint map rejected");
    assert_eq!(layout.checkpoints.len(), 2);
    assert_ne!(layout.checkpoints[0].carrier, layout.checkpoints[1].carrier);
    assert_eq!(config.grids.len(), 3);
}

#[test]
fn checkpoint_numbers_may_be_zero_or_shared_and_order_the_compiled_list() {
    let mut map = map_with_zones(4, vec![level(vec![[0, 0], [1, 0], [2, 0]])], Vec::new(), Vec::new());
    let numbered = |col, number| CheckpointDef {
        number,
        ..checkpoint_def(0, col, 0)
    };
    map.checkpoints = vec![numbered(0, 7), numbered(1, 0), numbered(2, 7)];
    validate_map(&map).expect("numbered checkpoints rejected");
    canonicalize(&mut map);
    let (layout, _) = compile_bare(&map).expect("numbered checkpoint compilation failed");
    assert_eq!(
        layout.checkpoints.iter().map(|c| c.number).collect::<Vec<_>>(),
        [0, 7, 7]
    );
    assert_eq!(layout.checkpoints[0].cols, [1, 2]);
}
