use super::*;
use crate::{
    map::{CellGrid, EdgeGrid},
    test_geometry::{LEVEL_HEIGHT, geometry},
};

fn level_with_walls(cols: i32, rows: i32) -> LevelGrid {
    // Single cell, all four sides walled. Lets us place lights on any
    // side without further setup.
    let mut edges = EdgeGrid::new(cols, rows);
    for col in 0..cols {
        edges.horizontal[0][col as usize] = true;
        edges.horizontal[rows as usize][col as usize] = true;
    }
    for row in 0..rows {
        edges.vertical[row as usize][0] = true;
        edges.vertical[row as usize][cols as usize] = true;
    }
    LevelGrid {
        cells: CellGrid::new(cols, rows),
        edges,
    }
}

#[test]
fn a_light_hangs_at_its_height_above_its_storeys_floor() {
    let level = level_with_walls(1, 1);
    let geometry = geometry(1, 1);
    let defs = vec![
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::North,
            height: 0.5,
        },
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::South,
            height: 2.5,
        },
    ];
    let lights = generate_wall_lights(&geometry, &level, 1, &defs, CarrierId::WORLD);
    assert_eq!(lights[0].pos.y, LEVEL_HEIGHT + 0.5);
    assert_eq!(lights[1].pos.y, LEVEL_HEIGHT + 2.5);
}

#[test]
fn a_light_off_the_grid_or_without_a_wall_on_its_side_is_dropped() {
    let mut level = level_with_walls(1, 1);
    level.edges.horizontal[0][0] = false;
    let light = |col, row, side| WallLightDef {
        kind: "test-light".into(),
        col,
        row,
        side,
        height: 2.5,
    };
    let defs = vec![
        light(0, 0, WallSide::North),
        light(5, 5, WallSide::North),
        light(0, 0, WallSide::South),
    ];

    let lights = generate_wall_lights(&geometry(1, 1), &level, 0, &defs, CarrierId::WORLD);

    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].yaw, PI);
}
