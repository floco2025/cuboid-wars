use super::*;

#[test]
fn checkpoints_require_valid_nonoverlapping_flat_floor_rectangles() {
    let mut map = map_with_zones(4, vec![level(vec![[0, 0], [1, 0]])], Vec::new(), Vec::new(), Vec::new());
    map.checkpoints.push(player_zone(0, 0, 0));
    validate_map(&map).expect("checkpoint map rejected");
    let compile = |map: &MapDef| compile_map(map, sizes(), &no_nested(), &empty_kind_table(), &no_bridges());
    let (layout, config) = compile(&map).expect("checkpoint compilation failed");
    assert_eq!(layout.checkpoints.len(), 1);
    assert_eq!(config.checkpoints.len(), 1);
    let checkpoint = layout.checkpoints[0];
    assert_eq!(checkpoint.min_x, config.root_grid().geometry.cell_to_world_x(0));
    assert_eq!(checkpoint.max_x, config.root_grid().geometry.cell_to_world_x(1));
    map.checkpoints.push(player_zone(0, 0, 0));
    assert!(
        validate_map(&map)
            .expect_err("overlapping checkpoints accepted")
            .to_string()
            .contains("overlaps")
    );
    map.checkpoints.pop();
    map.checkpoints[0].level = 3;
    assert!(validate_map(&map).is_err());
    map.checkpoints[0].level = 0;
    map.checkpoints[0].cols = [0, 3];
    assert!(
        validate_map(&map)
            .expect_err("checkpoint without flat floor accepted")
            .to_string()
            .contains("flat accessible floor")
    );
    map.checkpoints[0].cols = [0, 1];
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
    nested.checkpoints.push(player_zone(0, 0, 0));
    let mut root = map_with_zones(8, vec![level(Vec::new())], Vec::new(), Vec::new(), Vec::new());
    root.nested_maps = [0, 4]
        .map(|col| NestedMapDef {
            map: "platform".into(),
            motion: MotionDef {
                level: 0,
                from: [col, 0],
                to: [col, 3],
                to_level: None,
                travel_secs: 2.0,
                pause_secs: 0.0,
                phase_secs: 0.0,
                from_nudge: [0.0; 3],
                to_nudge: [0.0; 3],
            },
        })
        .to_vec();
    let (layout, config) = compile_map(
        &root,
        sizes(),
        &LoadedMaps::from([("platform".into(), nested)]),
        &empty_kind_table(),
        &no_bridges(),
    )
    .expect("nested checkpoint map rejected");
    assert_eq!(layout.checkpoints.len(), 2);
    assert_ne!(layout.checkpoints[0].carrier, layout.checkpoints[1].carrier);
    assert_eq!(config.checkpoints.len(), 2);
}
