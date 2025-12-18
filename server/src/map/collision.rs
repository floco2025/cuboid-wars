use common::{
    constants::*,
    protocol::{GridCell, Wall},
};

// Generate collision walls for roof edges to prevent players from falling off.
// Only adds edges where there's no adjacent roof or no ramp connection.
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
                    width: ROOF_WALL_THICKNESS,
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
                    width: ROOF_WALL_THICKNESS,
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
                    width: ROOF_WALL_THICKNESS,
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
                    width: ROOF_WALL_THICKNESS,
                });
            }
        }
    }

    roof_edge_walls
}
