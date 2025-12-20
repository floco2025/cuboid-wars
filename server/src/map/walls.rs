use crate::{constants::WALL_OVERLAP, resources::GridCell};
use common::{constants::*, protocol::Wall};

const MERGE_EPS: f32 = 0.01;

// Check if grid line has horizontal wall at position
#[inline]
fn has_horizontal_wall(grid: &[Vec<GridCell>], row: i32, col: i32, grid_rows: i32) -> bool {
    if row == 0 {
        grid[0][col as usize].has_north_wall
    } else if row == grid_rows {
        grid[(grid_rows - 1) as usize][col as usize].has_south_wall
    } else {
        grid[row as usize][col as usize].has_north_wall
    }
}

// Check if grid line has vertical wall at position
#[inline]
fn has_vertical_wall(grid: &[Vec<GridCell>], row: i32, col: i32, grid_cols: i32) -> bool {
    if col == 0 {
        grid[row as usize][0].has_west_wall
    } else if col == grid_cols {
        grid[row as usize][(grid_cols - 1) as usize].has_east_wall
    } else {
        grid[row as usize][col as usize].has_west_wall
    }
}

// Check if horizontal walls meet the top/bottom of a vertical wall
#[inline]
fn has_perpendicular_horizontal_walls(
    grid: &[Vec<GridCell>],
    row: i32,
    col: i32,
    grid_cols: i32,
    grid_rows: i32,
) -> (bool, bool) {
    // Top endpoint is at grid line `row`; check horizontal walls on both sides of the vertical line.
    let has_perp_top = row > 0
        && (
            (col < grid_cols && has_horizontal_wall(grid, row, col, grid_rows)) // right side (guarded)
                || (col > 0 && has_horizontal_wall(grid, row, col - 1, grid_rows))
            // left side (guarded)
        );

    // Bottom endpoint is at grid line `row + 1`; check the horizontals that meet there.
    let has_perp_bottom = row < grid_rows
        && (
            (col < grid_cols && has_horizontal_wall(grid, row + 1, col, grid_rows)) // right side (guarded)
                || (col > 0 && has_horizontal_wall(grid, row + 1, col - 1, grid_rows))
            // left side (guarded)
        );

    (has_perp_top, has_perp_bottom)
}

// Generate individual wall segments (no merging) with gap-filling extensions
#[must_use]
pub fn generate_walls(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Wall> {
    let mut walls = Vec::new();

    // Process horizontal walls (north/south edges)
    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !has_horizontal_wall(grid, row, col, grid_rows) {
                continue;
            }

            // Check for adjacent horizontal walls
            let has_left = col > 0 && has_horizontal_wall(grid, row, col - 1, grid_rows);
            let has_right = col < grid_cols - 1 && has_horizontal_wall(grid, row, col + 1, grid_rows);

            // Detect vertical walls passing through the left and right endpoints (true T vs corner)
            let left_vert_top = row > 0 && has_vertical_wall(grid, row - 1, col, grid_cols);
            let left_vert_bottom = row < grid_rows && has_vertical_wall(grid, row, col, grid_cols);
            let left_vert_through = left_vert_top && left_vert_bottom;

            let right_vert_top = row > 0 && has_vertical_wall(grid, row - 1, col + 1, grid_cols);
            let right_vert_bottom = row < grid_rows && has_vertical_wall(grid, row, col + 1, grid_cols);
            let right_vert_through = right_vert_top && right_vert_bottom;

            let world_z = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            // Horizontal walls inset only when a vertical passes through (T); extend otherwise for corners/ends
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                + if WALL_OVERLAP {
                    -WALL_THICKNESS / 2.0
                } else if left_vert_through && !has_left {
                    WALL_THICKNESS / 2.0 // inset at T so vertical can pass through
                } else if !has_left {
                    -WALL_THICKNESS / 2.0 // extend to meet corners or isolated ends
                } else {
                    0.0
                };
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                + if WALL_OVERLAP {
                    WALL_THICKNESS / 2.0
                } else if right_vert_through && !has_right {
                    -WALL_THICKNESS / 2.0 // inset at T on the right
                } else if !has_right {
                    WALL_THICKNESS / 2.0 // extend for corners or isolated ends
                } else {
                    0.0
                };

            walls.push(Wall {
                x1,
                z1: world_z,
                x2,
                z2: world_z,
                width: WALL_THICKNESS,
            });
        }
    }

    // Process vertical walls (west/east edges)
    for col in 0..=grid_cols {
        for row in 0..grid_rows {
            if !has_vertical_wall(grid, row, col, grid_cols) {
                continue;
            }

            // Check for adjacent vertical walls
            let has_top = row > 0 && has_vertical_wall(grid, row - 1, col, grid_cols);
            let has_bottom = row < grid_rows - 1 && has_vertical_wall(grid, row + 1, col, grid_cols);

            // Check for perpendicular horizontal walls at ends (for L-corners)
            let (has_perp_top, has_perp_bottom) =
                has_perpendicular_horizontal_walls(grid, row, col, grid_cols, grid_rows);

            let world_x = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0))
                + if has_perp_top && !has_top {
                    WALL_THICKNESS / 2.0 // Inset for L-corner
                } else if !has_top && !has_perp_top {
                    -WALL_THICKNESS / 2.0 // Extend for isolated end
                } else {
                    0.0
                };
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0))
                + if has_perp_bottom && !has_bottom {
                    -WALL_THICKNESS / 2.0 // Inset for L-corner
                } else if !has_bottom && !has_perp_bottom {
                    WALL_THICKNESS / 2.0 // Extend for isolated end
                } else {
                    0.0
                };

            walls.push(Wall {
                x1: world_x,
                z1,
                x2: world_x,
                z2,
                width: WALL_THICKNESS,
            });
        }
    }

    walls
}

// Normalize wall coordinates so they're in consistent order
fn normalize_wall(mut w: Wall) -> Wall {
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

// Merge adjacent collinear walls into longer segments
pub fn merge_walls(walls: Vec<Wall>) -> Vec<Wall> {
    let mut horizontals = Vec::new();
    let mut verticals = Vec::new();
    let mut others = Vec::new();

    for w in walls {
        let w = normalize_wall(w);
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
