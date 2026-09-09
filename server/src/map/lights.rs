use super::{
    definition::{WallLightDef, WallSide},
    edges::{CellSide, has_edge_on_cell_side},
};
use crate::map::LevelGrid;
use common::{
    map::MapGeometry,
    protocol::{CarrierId, Position, WallLight},
};
use std::f32::consts::{FRAC_PI_2, PI};

// Lamps hang this far in front of the wall face so the lamp body sits just
// inside the room, not flush with the wall texture.
const MODEL_INSET_PAST_WALL: f32 = 0.02;

// Turn the per-level manual `lights: [{col, row, side}]` entries from the
// map's `layout.json` into runtime `WallLight`s. Entries whose cell falls outside the
// grid or whose named side has no wall on this level are silently dropped —
// the editor's `canonicalize_map` filters those on save, this is just
// defense-in-depth for hand-edited JSON.
#[must_use]
pub(crate) fn generate_wall_lights(
    geometry: &MapGeometry,
    level: &LevelGrid,
    level_idx: usize,
    defs: &[WallLightDef],
    carrier: CarrierId,
) -> Vec<WallLight> {
    let light_y = geometry.level_y(u8::try_from(level_idx).unwrap_or(u8::MAX)) + geometry.wall_light_height();

    defs.iter()
        .filter_map(|def| {
            if !cell_in_bounds(level, def.row, def.col) {
                return None;
            }
            let side = cell_side_from_wall_side(def.side);
            if !has_edge_on_cell_side(&level.edges, def.row, def.col, side) {
                return None;
            }
            let (pos, yaw) = wall_light_pose(geometry, light_y, def.row, def.col, side);
            Some(WallLight {
                kind: def.kind.clone(),
                pos,
                yaw,
                carrier,
            })
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

// Where a lamp on `side` of the cell hangs, and which way it faces.
fn wall_light_pose(geometry: &MapGeometry, light_y: f32, row: i32, col: i32, side: CellSide) -> (Position, f32) {
    let cell_center_x = geometry.cell_center_x(col);
    let cell_center_z = geometry.cell_center_z(row);
    let half = geometry.cell_size() / 2.0;
    let model_inset = geometry.wall_half_thickness() + MODEL_INSET_PAST_WALL;
    let (x, z, yaw) = match side {
        CellSide::North => (cell_center_x, cell_center_z - half + model_inset, 0.0),
        CellSide::South => (cell_center_x, cell_center_z + half - model_inset, PI),
        CellSide::West => (cell_center_x - half + model_inset, cell_center_z, FRAC_PI_2),
        CellSide::East => (cell_center_x + half - model_inset, cell_center_z, -FRAC_PI_2),
    };
    (Position { x, y: light_y, z }, yaw)
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
}
