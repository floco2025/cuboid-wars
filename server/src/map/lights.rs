use crate::{constants::WALL_LIGHT_HEIGHT, resources::GridCell};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_COLS, GRID_ROWS, GRID_SIZE},
    protocol::{Position, Wall, WallLight},
};

const LOCAL_Z_PUSH: f32 = 0.1; // push fixture off the wall
const FORWARD_OFFSET: f32 = 0.05; // point light nudge outward

pub fn generate_wall_lights(grid: &[Vec<GridCell>], walls: &[Wall]) -> Vec<WallLight> {
    let mut lights = Vec::new();

    for wall in walls {
        let center_x = f32::midpoint(wall.x1, wall.x2);
        let center_z = f32::midpoint(wall.z1, wall.z2);

        let dx = wall.x2 - wall.x1;
        let dz = wall.z2 - wall.z1;
        let rotation_y = dz.atan2(dx);

        let mesh_size_z = wall.width;
        let is_horizontal = dx.abs() > dz.abs();

        if is_horizontal {
            let row_line = ((center_z + FIELD_DEPTH / 2.0) / GRID_SIZE).round() as i32;
            if row_line < 0 || row_line > GRID_ROWS {
                continue;
            }

            let x_min = wall.x1.min(wall.x2);
            let x_max = wall.x1.max(wall.x2);

            let start_col_line = (((x_min + FIELD_WIDTH / 2.0) / GRID_SIZE).round() as i32).clamp(0, GRID_COLS);
            let end_col_line = (((x_max + FIELD_WIDTH / 2.0) / GRID_SIZE).round() as i32).clamp(0, GRID_COLS);

            for col_line in start_col_line..end_col_line {
                if col_line < 0 || col_line >= GRID_COLS {
                    continue;
                }

                let grid_center_x = (col_line as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let (needs_pos, needs_neg) = light_sides_for_horizontal(row_line, col_line, grid);

                if needs_pos {
                    push_light(
                        &mut lights,
                        grid_center_x,
                        center_z,
                        center_x,
                        center_z,
                        rotation_y,
                        mesh_size_z,
                        1.0,
                    );
                }

                if needs_neg {
                    push_light(
                        &mut lights,
                        grid_center_x,
                        center_z,
                        center_x,
                        center_z,
                        rotation_y,
                        mesh_size_z,
                        -1.0,
                    );
                }
            }
        } else {
            let col_line = ((center_x + FIELD_WIDTH / 2.0) / GRID_SIZE).round() as i32;
            if col_line < 0 || col_line > GRID_COLS {
                continue;
            }

            let z_min = wall.z1.min(wall.z2);
            let z_max = wall.z1.max(wall.z2);

            let start_row_line = (((z_min + FIELD_DEPTH / 2.0) / GRID_SIZE).round() as i32).clamp(0, GRID_ROWS);
            let end_row_line = (((z_max + FIELD_DEPTH / 2.0) / GRID_SIZE).round() as i32).clamp(0, GRID_ROWS);

            for row_line in start_row_line..end_row_line {
                if row_line < 0 || row_line >= GRID_ROWS {
                    continue;
                }

                let grid_center_z = (row_line as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                let (needs_pos, needs_neg) = light_sides_for_vertical(row_line, col_line, grid);

                if needs_pos {
                    // East side needs a negative local Z to land on +X due to rotation convention
                    push_light(
                        &mut lights,
                        center_x,
                        grid_center_z,
                        center_x,
                        center_z,
                        rotation_y,
                        mesh_size_z,
                        -1.0,
                    );
                }

                if needs_neg {
                    // West side uses positive local Z (default)
                    push_light(
                        &mut lights,
                        center_x,
                        grid_center_z,
                        center_x,
                        center_z,
                        rotation_y,
                        mesh_size_z,
                        1.0,
                    );
                }
            }
        }
    }

    lights
}

fn push_light(
    lights: &mut Vec<WallLight>,
    world_x: f32,
    world_z: f32,
    wall_center_x: f32,
    wall_center_z: f32,
    wall_yaw: f32,
    wall_width: f32,
    side_sign: f32,
) {
    let (sin_yaw, cos_yaw) = wall_yaw.sin_cos();
    let world_dx = world_x - wall_center_x;
    let world_dz = world_z - wall_center_z;
    let local_x = world_dx * cos_yaw + world_dz * sin_yaw;

    // Place both the fixture and the emitted light at the absolute height; the wall itself is centered at y = WALL_HEIGHT/2.
    let local_y = WALL_LIGHT_HEIGHT;
    let local_z = side_sign * (wall_width / 2.0 + LOCAL_Z_PUSH);
    let forward = side_sign * FORWARD_OFFSET;
    let model_world_x = wall_center_x + local_x * cos_yaw - local_z * sin_yaw;
    let model_world_z = wall_center_z + local_x * sin_yaw + local_z * cos_yaw;

    let light_z = local_z + forward;
    let light_world_x = wall_center_x + local_x * cos_yaw - light_z * sin_yaw;
    let light_world_z = wall_center_z + local_x * sin_yaw + light_z * cos_yaw;

    let yaw = if side_sign >= 0.0 {
        wall_yaw
    } else {
        wall_yaw + std::f32::consts::PI
    };

    lights.push(WallLight {
        model_pos: Position {
            x: model_world_x,
            y: local_y,
            z: model_world_z,
        },
        light_pos: Position {
            x: light_world_x,
            y: local_y,
            z: light_world_z,
        },
        yaw,
    });
}

fn light_sides_for_horizontal(row_line: i32, col: i32, grid: &[Vec<GridCell>]) -> (bool, bool) {
    if col < 0 || col >= GRID_COLS {
        return (false, false);
    }

    let south_roof = if row_line >= 0 && row_line < GRID_ROWS {
        grid[row_line as usize][col as usize].has_roof
    } else {
        false
    };

    let north_roof = if row_line > 0 && row_line <= GRID_ROWS {
        grid[(row_line - 1) as usize][col as usize].has_roof
    } else {
        false
    };

    (south_roof, north_roof)
}

fn light_sides_for_vertical(row: i32, col_line: i32, grid: &[Vec<GridCell>]) -> (bool, bool) {
    if row < 0 || row >= GRID_ROWS {
        return (false, false);
    }

    let east_roof = if col_line >= 0 && col_line < GRID_COLS {
        grid[row as usize][col_line as usize].has_roof
    } else {
        false
    };

    let west_roof = if col_line > 0 && col_line <= GRID_COLS {
        grid[row as usize][(col_line - 1) as usize].has_roof
    } else {
        false
    };

    (east_roof, west_roof)
}
