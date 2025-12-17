use crate::resources::GridCell;
use common::{
    constants::*,
    protocol::{Ramp, Wall},
};

/// Generate collision walls for ramps.
/// Returns (ramp_side_walls, ramp_all_walls):
/// - ramp_side_walls: Only the sides perpendicular to slope (for players)
/// - ramp_all_walls: All four edges (for ghosts)
pub fn generate_ramp_collision_walls(ramps: &[Ramp], _grid: &[Vec<GridCell>]) -> (Vec<Wall>, Vec<Wall>) {
    let mut ramp_side_walls = Vec::new();
    let mut ramp_all_walls = Vec::new();

    for ramp in ramps {
        // Determine ramp direction from low (x1, z1) to high (x2, z2)
        let dx = ramp.x2 - ramp.x1;
        let dz = ramp.z2 - ramp.z1;

        // Ramp footprint boundaries (axis-aligned bounding box)
        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);

        // Determine if ramp runs along X or Z axis
        let runs_along_x = dx.abs() > dz.abs();

        if runs_along_x {
            // Ramp runs along X axis, sides are perpendicular (along Z)
            // Side walls at constant Z: min_z and max_z
            let side_wall_1 = Wall {
                x1: min_x,
                z1: min_z,
                x2: max_x,
                z2: min_z,
                width: WALL_WIDTH,
            };
            let side_wall_2 = Wall {
                x1: min_x,
                z1: max_z,
                x2: max_x,
                z2: max_z,
                width: WALL_WIDTH,
            };
            ramp_side_walls.push(side_wall_1);
            ramp_side_walls.push(side_wall_2);

            // Entry/exit walls at constant X: min_x and max_x
            let end_wall_1 = Wall {
                x1: min_x,
                z1: min_z,
                x2: min_x,
                z2: max_z,
                width: WALL_WIDTH,
            };
            let end_wall_2 = Wall {
                x1: max_x,
                z1: min_z,
                x2: max_x,
                z2: max_z,
                width: WALL_WIDTH,
            };

            // Add all four walls to ramp_all_walls
            ramp_all_walls.push(side_wall_1);
            ramp_all_walls.push(side_wall_2);
            ramp_all_walls.push(end_wall_1);
            ramp_all_walls.push(end_wall_2);
        } else {
            // Ramp runs along Z axis, sides are perpendicular (along X)
            // Side walls at constant X: min_x and max_x
            let side_wall_1 = Wall {
                x1: min_x,
                z1: min_z,
                x2: min_x,
                z2: max_z,
                width: WALL_WIDTH,
            };
            let side_wall_2 = Wall {
                x1: max_x,
                z1: min_z,
                x2: max_x,
                z2: max_z,
                width: WALL_WIDTH,
            };
            ramp_side_walls.push(side_wall_1);
            ramp_side_walls.push(side_wall_2);

            // Entry/exit walls at constant Z: min_z and max_z
            let end_wall_1 = Wall {
                x1: min_x,
                z1: min_z,
                x2: max_x,
                z2: min_z,
                width: WALL_WIDTH,
            };
            let end_wall_2 = Wall {
                x1: min_x,
                z1: max_z,
                x2: max_x,
                z2: max_z,
                width: WALL_WIDTH,
            };

            // Add all four walls to ramp_all_walls
            ramp_all_walls.push(side_wall_1);
            ramp_all_walls.push(side_wall_2);
            ramp_all_walls.push(end_wall_1);
            ramp_all_walls.push(end_wall_2);
        }
    }

    (ramp_side_walls, ramp_all_walls)
}

/// Generate collision walls for roof edges to prevent players from falling off.
/// Only adds edges where there's no adjacent roof or no ramp connection.
pub fn generate_roof_edge_walls(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Wall> {
    let mut roof_edge_walls = Vec::new();

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = grid[row as usize][col as usize];
            if !cell.has_roof {
                continue;
            }

            // Calculate cell boundaries in world coordinates
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            // Check each edge - add wall if no adjacent roof and no ramp connection
            // North edge (z1) - check if neighbor to the north has a ramp_top_south (connecting upward to this roof)
            let has_north_neighbor_roof = row > 0 && grid[(row - 1) as usize][col as usize].has_roof;
            let has_north_ramp = row > 0 && grid[(row - 1) as usize][col as usize].ramp_top_south;
            if !has_north_neighbor_roof && !has_north_ramp {
                roof_edge_walls.push(Wall {
                    x1,
                    z1,
                    x2,
                    z2: z1,
                    width: WALL_WIDTH,
                });
            }

            // South edge (z2) - check if neighbor to the south has a ramp_top_north
            let has_south_neighbor_roof = row < grid_rows - 1 && grid[(row + 1) as usize][col as usize].has_roof;
            let has_south_ramp = row < grid_rows - 1 && grid[(row + 1) as usize][col as usize].ramp_top_north;
            if !has_south_neighbor_roof && !has_south_ramp {
                roof_edge_walls.push(Wall {
                    x1,
                    z1: z2,
                    x2,
                    z2,
                    width: WALL_WIDTH,
                });
            }

            // West edge (x1) - check if neighbor to the west has a ramp_top_east
            let has_west_neighbor_roof = col > 0 && grid[row as usize][(col - 1) as usize].has_roof;
            let has_west_ramp = col > 0 && grid[row as usize][(col - 1) as usize].ramp_top_east;
            if !has_west_neighbor_roof && !has_west_ramp {
                roof_edge_walls.push(Wall {
                    x1,
                    z1,
                    x2: x1,
                    z2,
                    width: WALL_WIDTH,
                });
            }

            // East edge (x2) - check if neighbor to the east has a ramp_top_west
            let has_east_neighbor_roof = col < grid_cols - 1 && grid[row as usize][(col + 1) as usize].has_roof;
            let has_east_ramp = col < grid_cols - 1 && grid[row as usize][(col + 1) as usize].ramp_top_west;
            if !has_east_neighbor_roof && !has_east_ramp {
                roof_edge_walls.push(Wall {
                    x1: x2,
                    z1,
                    x2,
                    z2,
                    width: WALL_WIDTH,
                });
            }
        }
    }

    roof_edge_walls
}
