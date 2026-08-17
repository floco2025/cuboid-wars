use super::definition::{WallLightDef, WallSide};
use super::edges::{CellSide, has_edge_on_cell_side};
use crate::map::LevelGrid;
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT, WALL_HALF_THICKNESS},
    map::MapGeometry,
    protocol::{Position, WallLight},
};
use std::f32::consts::{FRAC_PI_2, PI};

// Vertical position of every wall lamp, measured from the floor it sits on.
// Lamps are placed manually in the editor (per-level `lights` arrays in
// `map.json`); this constant sets their height above the level floor.
const WALL_LIGHT_HEIGHT: f32 = 2.5;

// Lamps hang `MODEL_INSET` in front of the wall face so the lamp body sits
// just inside the room, not flush with the wall texture.
const MODEL_INSET: f32 = WALL_HALF_THICKNESS + 0.02;

// Turn the per-level manual `lights: [{col, row, side}]` entries from
// `map.json` into runtime `WallLight`s. Entries whose cell falls outside the
// grid or whose named side has no wall on this level are silently dropped —
// the editor's `canonicalize_map` filters those on save, this is just
// defense-in-depth for hand-edited JSON.
#[must_use]
pub(crate) fn generate_wall_lights(
    geometry: &MapGeometry,
    level: &LevelGrid,
    level_idx: usize,
    defs: &[WallLightDef],
) -> Vec<WallLight> {
    let light_y = (level_idx as f32).mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT);

    defs.iter()
        .filter_map(|def| {
            if !cell_in_bounds(level, def.row, def.col) {
                return None;
            }
            let side = cell_side_from_wall_side(def.side);
            if !has_edge_on_cell_side(&level.edges, def.row, def.col, side) {
                return None;
            }
            Some(wall_light_for(geometry, light_y, def.row, def.col, side))
        })
        .collect()
}

fn cell_side_from_wall_side(side: WallSide) -> CellSide {
    match side {
        WallSide::North => CellSide::North,
        WallSide::South => CellSide::South,
        WallSide::East => CellSide::East,
        WallSide::West => CellSide::West,
    }
}

fn wall_light_for(geometry: &MapGeometry, light_y: f32, row: i32, col: i32, side: CellSide) -> WallLight {
    let cell_center_x = geometry.cell_to_world_x(col) + GRID_CELL_SIZE / 2.0;
    let cell_center_z = geometry.cell_to_world_z(row) + GRID_CELL_SIZE / 2.0;
    let half = GRID_CELL_SIZE / 2.0;
    match side {
        CellSide::North => WallLight {
            pos: Position {
                x: cell_center_x,
                y: light_y,
                z: cell_center_z - half + MODEL_INSET,
            },
            yaw: 0.0,
        },
        CellSide::South => WallLight {
            pos: Position {
                x: cell_center_x,
                y: light_y,
                z: cell_center_z + half - MODEL_INSET,
            },
            yaw: PI,
        },
        CellSide::West => WallLight {
            pos: Position {
                x: cell_center_x - half + MODEL_INSET,
                y: light_y,
                z: cell_center_z,
            },
            yaw: FRAC_PI_2,
        },
        CellSide::East => WallLight {
            pos: Position {
                x: cell_center_x + half - MODEL_INSET,
                y: light_y,
                z: cell_center_z,
            },
            yaw: -FRAC_PI_2,
        },
    }
}

fn cell_in_bounds(level: &LevelGrid, row: i32, col: i32) -> bool {
    row >= 0
        && col >= 0
        && level
            .cells
            .rows
            .get(row as usize)
            .is_some_and(|grid_row| grid_row.get(col as usize).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{CellGrid, EdgeGrid};

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
                col: 0,
                row: 0,
                side: WallSide::North,
            },
            WallLightDef {
                col: 0,
                row: 0,
                side: WallSide::South,
            },
            WallLightDef {
                col: 0,
                row: 0,
                side: WallSide::East,
            },
            WallLightDef {
                col: 0,
                row: 0,
                side: WallSide::West,
            },
        ];

        let lights = generate_wall_lights(&MapGeometry::new(1, 1), &level, 0, &defs);

        assert_eq!(lights.len(), 4);
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
            col: 0,
            row: 0,
            side: WallSide::North,
        }];

        let lights = generate_wall_lights(&MapGeometry::new(1, 1), &level, 2, &defs);

        assert_eq!(lights.len(), 1);
        assert!((lights[0].pos.y - 2.0_f32.mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT)).abs() < 1e-5);
    }

    #[test]
    fn drops_def_without_a_wall_on_the_named_side() {
        let mut level = level_with_walls(1, 1);
        level.edges.horizontal[0][0] = false; // remove north wall
        let defs = vec![
            WallLightDef {
                col: 0,
                row: 0,
                side: WallSide::North,
            },
            WallLightDef {
                col: 0,
                row: 0,
                side: WallSide::South,
            },
        ];

        let lights = generate_wall_lights(&MapGeometry::new(1, 1), &level, 0, &defs);

        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].yaw, PI);
    }

    #[test]
    fn drops_def_with_out_of_bounds_cell() {
        let level = level_with_walls(1, 1);
        let defs = vec![WallLightDef {
            col: 5,
            row: 5,
            side: WallSide::North,
        }];

        let lights = generate_wall_lights(&MapGeometry::new(1, 1), &level, 0, &defs);

        assert!(lights.is_empty());
    }
}
