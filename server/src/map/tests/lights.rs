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
        barrier_edges: EdgeGrid::new(cols, rows),
    }
}

#[test]
fn places_one_light_per_def_with_correct_yaw() {
    let level = level_with_walls(1, 1);
    let defs = vec![
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::North,
        },
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::South,
        },
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::East,
        },
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::West,
        },
    ];

    let lights = generate_wall_lights(&geometry(1, 1), &level, 0, &defs, CarrierId::WORLD);

    assert_eq!(lights.len(), 4);
    assert!(lights.iter().all(|light| light.kind == "test-light"));
    let yaws: Vec<f32> = lights.iter().map(|l| l.yaw).collect();
    assert!(yaws.contains(&0.0));
    assert!(yaws.contains(&PI));
    assert!(yaws.contains(&FRAC_PI_2));
    assert!(yaws.contains(&-FRAC_PI_2));
}

#[test]
fn light_y_uses_level_offset() {
    let level = level_with_walls(1, 1);
    let defs = vec![WallLightDef {
        kind: "test-light".into(),
        col: 0,
        row: 0,
        side: WallSide::North,
    }];

    let geometry = geometry(1, 1);
    let lights = generate_wall_lights(&geometry, &level, 2, &defs, CarrierId::WORLD);

    assert_eq!(lights.len(), 1);
    assert!((lights[0].pos.y - (2.0 * LEVEL_HEIGHT + geometry.wall_light_height())).abs() < 1e-5);
}

#[test]
fn drops_def_without_a_wall_on_the_named_side() {
    let mut level = level_with_walls(1, 1);
    level.edges.horizontal[0][0] = false; // remove north wall
    let defs = vec![
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::North,
        },
        WallLightDef {
            kind: "test-light".into(),
            col: 0,
            row: 0,
            side: WallSide::South,
        },
    ];

    let lights = generate_wall_lights(&geometry(1, 1), &level, 0, &defs, CarrierId::WORLD);

    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].yaw, PI);
}

#[test]
fn drops_def_with_out_of_bounds_cell() {
    let level = level_with_walls(1, 1);
    let defs = vec![WallLightDef {
        kind: "test-light".into(),
        col: 5,
        row: 5,
        side: WallSide::North,
    }];

    let lights = generate_wall_lights(&geometry(1, 1), &level, 0, &defs, CarrierId::WORLD);

    assert!(lights.is_empty());
}
