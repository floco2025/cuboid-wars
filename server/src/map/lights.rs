use crate::{constants::WALL_LIGHT_HEIGHT, resources::GridCell};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE, WALL_THICKNESS},
    protocol::{Position, WallLight},
};

const MODEL_INSET: f32 = WALL_THICKNESS / 2.0 + 0.02; // place fixture just inside the cell away from wall surface

pub fn generate_wall_lights(grid: &[Vec<GridCell>]) -> Vec<WallLight> {
    let mut lights = Vec::new();

    let grid_rows = grid.len() as i32;
    let grid_cols = grid.first().map_or(0, Vec::len) as i32;

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = &grid[row as usize][col as usize];
            if !cell.has_roof {
                continue;
            }

            let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let half = GRID_SIZE / 2.0;

            // North wall: place on top edge, face inward (+Z)
            if cell.has_north_wall {
                let base_z = cell_center_z - half;
                let model_z = base_z + MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: WALL_LIGHT_HEIGHT,
                        z: model_z,
                    },
                    yaw: 0.0,
                });
            }

            // South wall: bottom edge, face inward (-Z)
            if cell.has_south_wall {
                let base_z = cell_center_z + half;
                let model_z = base_z - MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: WALL_LIGHT_HEIGHT,
                        z: model_z,
                    },
                    yaw: std::f32::consts::PI,
                });
            }

            // West wall: left edge, face inward (+X)
            if cell.has_west_wall {
                let base_x = cell_center_x - half;
                let model_x = base_x + MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: model_x,
                        y: WALL_LIGHT_HEIGHT,
                        z: cell_center_z,
                    },
                    yaw: std::f32::consts::FRAC_PI_2,
                });
            }

            // East wall: right edge, face inward (-X)
            if cell.has_east_wall {
                let base_x = cell_center_x + half;
                let model_x = base_x - MODEL_INSET;
                lights.push(WallLight {
                    pos: Position {
                        x: model_x,
                        y: WALL_LIGHT_HEIGHT,
                        z: cell_center_z,
                    },
                    yaw: -std::f32::consts::FRAC_PI_2,
                });
            }
        }
    }

    lights
}
