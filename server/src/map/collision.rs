use crate::resources::GridCell;
use common::{constants::*, protocol::Wall};

const MERGE_EPS: f32 = 0.01;

// Generate collision walls for roof edges to prevent players from falling off.
// Only adds edges where there's no adjacent roof or no ramp connection.
pub fn generate_roof_walls(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Wall> {
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

// Normalize wall coordinates so they're in consistent order
fn normalize_roof_wall(mut w: Wall) -> Wall {
    if (w.z1 - w.z2).abs() < MERGE_EPS {
        // horizontal: order by x
        if w.x1 > w.x2 {
            std::mem::swap(&mut w.x1, &mut w.x2);
        }
    } else if (w.x1 - w.x2).abs() < MERGE_EPS {
        // vertical: order by z
        if w.z1 > w.z2 {
            std::mem::swap(&mut w.z1, &mut w.z2);
        }
    }
    w
}

// Merge adjacent collinear roof walls into longer segments
pub fn merge_roof_walls(walls: Vec<Wall>) -> Vec<Wall> {
    let mut horizontals = Vec::new();
    let mut verticals = Vec::new();
    let mut others = Vec::new();

    for w in walls {
        let w = normalize_roof_wall(w);
        if (w.z1 - w.z2).abs() < MERGE_EPS {
            horizontals.push(w);
        } else if (w.x1 - w.x2).abs() < MERGE_EPS {
            verticals.push(w);
        } else {
            others.push(w);
        }
    }

    horizontals.sort_by(|a, b| {
        a.z1.partial_cmp(&b.z1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|a, b| {
        a.x1.partial_cmp(&b.x1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged = Vec::new();

    let merge_line = |list: Vec<Wall>, is_horizontal: bool, out: &mut Vec<Wall>| {
        let mut iter = list.into_iter();
        if let Some(mut cur) = iter.next() {
            for w in iter {
                if is_horizontal {
                    if (cur.z1 - w.z1).abs() < MERGE_EPS
                        && (cur.width - w.width).abs() < MERGE_EPS
                        && w.x1 <= cur.x2 + MERGE_EPS
                    {
                        cur.x2 = cur.x2.max(w.x2);
                        continue;
                    }
                } else if (cur.x1 - w.x1).abs() < MERGE_EPS
                    && (cur.width - w.width).abs() < MERGE_EPS
                    && w.z1 <= cur.z2 + MERGE_EPS
                {
                    cur.z2 = cur.z2.max(w.z2);
                    continue;
                }
                out.push(cur);
                cur = w;
            }
            out.push(cur);
        }
    };

    merge_line(horizontals, true, &mut merged);
    merge_line(verticals, false, &mut merged);
    merged.extend(others);
    merged
}
