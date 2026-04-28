use super::edges::{CellSide, has_edge_on_cell_side};
use crate::{
    constants::WALL_LIGHT_HEIGHT,
    resources::{CellGrid, EdgeGrid, LevelGrid},
};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE, LEVEL_HEIGHT, WALL_THICKNESS},
    protocol::{Position, WallLight},
};

const MODEL_INSET: f32 = WALL_THICKNESS / 2.0 + 0.02; // place fixture just inside the cell away from wall surface

#[must_use]
pub fn generate_wall_lights(level_grid: &LevelGrid, level: u8) -> Vec<WallLight> {
    generate_wall_lights_from_parts(&level_grid.cells, &level_grid.edges, level)
}

fn generate_wall_lights_from_parts(cells: &CellGrid, edges: &EdgeGrid, level: u8) -> Vec<WallLight> {
    let mut lights = Vec::new();

    let grid_rows = cells.rows.len() as i32;
    let grid_cols = cells.rows.first().map_or(0, Vec::len) as i32;
    let light_y = f32::from(level).mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT);

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = &cells.rows[row as usize][col as usize];
            if !cell.has_floor_above {
                continue;
            }

            let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let half = GRID_SIZE / 2.0;

            // North wall: place on top edge, face inward (+Z)
            if has_edge_on_cell_side(edges, row, col, CellSide::North) {
                let base_z = cell_center_z - half;
                let model_z = base_z + MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: light_y,
                        z: model_z,
                    },
                    yaw: 0.0,
                });
            }

            // South wall: bottom edge, face inward (-Z)
            if has_edge_on_cell_side(edges, row, col, CellSide::South) {
                let base_z = cell_center_z + half;
                let model_z = base_z - MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: light_y,
                        z: model_z,
                    },
                    yaw: std::f32::consts::PI,
                });
            }

            // West wall: left edge, face inward (+X)
            if has_edge_on_cell_side(edges, row, col, CellSide::West) {
                let base_x = cell_center_x - half;
                let model_x = base_x + MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: model_x,
                        y: light_y,
                        z: cell_center_z,
                    },
                    yaw: std::f32::consts::FRAC_PI_2,
                });
            }

            // East wall: right edge, face inward (-X)
            if has_edge_on_cell_side(edges, row, col, CellSide::East) {
                let base_x = cell_center_x + half;
                let model_x = base_x - MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: model_x,
                        y: light_y,
                        z: cell_center_z,
                    },
                    yaw: -std::f32::consts::FRAC_PI_2,
                });
            }
        }
    }

    lights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_lights_use_level_height_offset() {
        let mut cells = CellGrid::new(1, 1);
        let mut edges = EdgeGrid::new(1, 1);
        cells.rows[0][0].has_floor_above = true;
        edges.horizontal[0][0] = true;

        let lights = generate_wall_lights_from_parts(&cells, &edges, 2);

        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].pos.y, 2.0_f32.mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT));
    }

    #[test]
    fn wall_lights_require_floor_above() {
        let cells = CellGrid::new(1, 1);
        let mut edges = EdgeGrid::new(1, 1);
        edges.horizontal[0][0] = true;

        let lights = generate_wall_lights_from_parts(&cells, &edges, 0);

        assert!(lights.is_empty());
    }

    #[test]
    fn internal_horizontal_wall_gets_lights_on_both_sides() {
        let mut cells = CellGrid::new(1, 2);
        let mut edges = EdgeGrid::new(1, 2);
        cells.rows[0][0].has_floor_above = true;
        cells.rows[1][0].has_floor_above = true;
        edges.horizontal[1][0] = true;

        let lights = generate_wall_lights_from_parts(&cells, &edges, 0);

        assert_eq!(lights.len(), 2);
        assert!(lights.iter().any(|light| light.yaw == 0.0));
        assert!(lights.iter().any(|light| light.yaw == std::f32::consts::PI));
    }

    #[test]
    fn internal_vertical_wall_gets_lights_on_both_sides() {
        let mut cells = CellGrid::new(2, 1);
        let mut edges = EdgeGrid::new(2, 1);
        cells.rows[0][0].has_floor_above = true;
        cells.rows[0][1].has_floor_above = true;
        edges.vertical[0][1] = true;

        let lights = generate_wall_lights_from_parts(&cells, &edges, 0);

        assert_eq!(lights.len(), 2);
        assert!(lights.iter().any(|light| light.yaw == std::f32::consts::FRAC_PI_2));
        assert!(lights.iter().any(|light| light.yaw == -std::f32::consts::FRAC_PI_2));
    }
}
