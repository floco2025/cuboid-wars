use crate::{
    constants::{GRID_CELL_SIZE, GRID_COLS, GRID_ROWS, LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    protocol::{Floor, Ramp, Wall},
};

pub(super) fn floor_cells(floor: &Floor) -> Vec<(i32, i32)> {
    let min_x = floor.x1.min(floor.x2);
    let max_x = floor.x1.max(floor.x2);
    let min_z = floor.z1.min(floor.z2);
    let max_z = floor.z1.max(floor.z2);

    let min_col = first_cell_center_at_or_after(min_x, MAP_WIDTH, GRID_COLS);
    let max_col = last_cell_center_at_or_before(max_x, MAP_WIDTH, GRID_COLS);
    let min_row = first_cell_center_at_or_after(min_z, MAP_DEPTH, GRID_ROWS);
    let max_row = last_cell_center_at_or_before(max_z, MAP_DEPTH, GRID_ROWS);

    let mut cells = Vec::new();
    if min_col > max_col || min_row > max_row {
        return cells;
    }
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            cells.push((col, row));
        }
    }
    cells
}

pub(super) fn ramp_cells(ramp: &Ramp) -> Vec<(i32, i32)> {
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    let min_col = first_cell_center_at_or_after(min_x, MAP_WIDTH, GRID_COLS);
    let max_col = last_cell_center_at_or_before(max_x, MAP_WIDTH, GRID_COLS);
    let min_row = first_cell_center_at_or_after(min_z, MAP_DEPTH, GRID_ROWS);
    let max_row = last_cell_center_at_or_before(max_z, MAP_DEPTH, GRID_ROWS);

    let mut cells = Vec::new();
    if min_col > max_col || min_row > max_row {
        return cells;
    }
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            cells.push((col, row));
        }
    }
    cells
}

pub(super) fn wall_edges(wall: &Wall) -> Vec<([i32; 2], [i32; 2])> {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    if dx >= dz {
        let row = world_z_to_grid_row(f32::midpoint(wall.z1, wall.z2));
        let min_col = first_edge_midpoint_at_or_after(wall.x1.min(wall.x2), MAP_WIDTH, GRID_COLS);
        let max_col = last_edge_midpoint_at_or_before(wall.x1.max(wall.x2), MAP_WIDTH, GRID_COLS);
        if min_col > max_col {
            return Vec::new();
        }
        return (min_col..=max_col).map(|col| ([col, row], [col + 1, row])).collect();
    }

    let col = world_x_to_grid_col(f32::midpoint(wall.x1, wall.x2));
    let min_row = first_edge_midpoint_at_or_after(wall.z1.min(wall.z2), MAP_DEPTH, GRID_ROWS);
    let max_row = last_edge_midpoint_at_or_before(wall.z1.max(wall.z2), MAP_DEPTH, GRID_ROWS);
    if min_row > max_row {
        return Vec::new();
    }
    (min_row..=max_row).map(|row| ([col, row], [col, row + 1])).collect()
}

fn first_cell_center_at_or_after(world: f32, map_size: f32, count: i32) -> i32 {
    (((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).ceil() as i32).clamp(0, count - 1)
}

fn last_cell_center_at_or_before(world: f32, map_size: f32, count: i32) -> i32 {
    (((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).floor() as i32).clamp(0, count - 1)
}

fn first_edge_midpoint_at_or_after(world: f32, map_size: f32, edge_count: i32) -> i32 {
    first_cell_center_at_or_after(world, map_size, edge_count)
}

fn last_edge_midpoint_at_or_before(world: f32, map_size: f32, edge_count: i32) -> i32 {
    last_cell_center_at_or_before(world, map_size, edge_count)
}

#[must_use]
pub fn world_x_to_grid_col(x: f32) -> i32 {
    ((x + MAP_WIDTH / 2.0) / GRID_CELL_SIZE).round() as i32
}

#[must_use]
pub fn world_z_to_grid_row(z: f32) -> i32 {
    ((z + MAP_DEPTH / 2.0) / GRID_CELL_SIZE).round() as i32
}

pub(super) fn world_x_to_cell_col(x: f32) -> i32 {
    ((x + MAP_WIDTH / 2.0) / GRID_CELL_SIZE)
        .floor()
        .clamp(0.0, (GRID_COLS - 1) as f32) as i32
}

pub(super) fn world_z_to_cell_row(z: f32) -> i32 {
    ((z + MAP_DEPTH / 2.0) / GRID_CELL_SIZE)
        .floor()
        .clamp(0.0, (GRID_ROWS - 1) as f32) as i32
}

#[must_use]
pub fn grid_col_to_world_x(col: i32) -> f32 {
    (col as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0))
}

#[must_use]
pub fn grid_row_to_world_z(row: i32) -> f32 {
    (row as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0))
}

pub(super) fn ramp_lower_level(ramp: &Ramp) -> u8 {
    let lower_y = ramp.y1.min(ramp.y2);
    (lower_y / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8
}

pub(super) fn same_edge(rule_from: [i32; 2], rule_to: [i32; 2], from: [i32; 2], to: [i32; 2]) -> bool {
    (rule_from == from && rule_to == to) || (rule_from == to && rule_to == from)
}

pub(super) fn touches_vertical_line(from: [i32; 2], to: [i32; 2], col: i32, [row_min, row_max]: [i32; 2]) -> bool {
    let is_horizontal_edge = from[1] == to[1] && from[0] != to[0];
    let row = from[1];
    is_horizontal_edge
        && (row_min..=row_max).contains(&row)
        && (from[0] == col || to[0] == col)
        && !(from[0] == col && to[0] == col)
}

pub(super) fn touches_horizontal_line(from: [i32; 2], to: [i32; 2], row: i32, [col_min, col_max]: [i32; 2]) -> bool {
    let is_vertical_edge = from[0] == to[0] && from[1] != to[1];
    let col = from[0];
    is_vertical_edge
        && (col_min..=col_max).contains(&col)
        && (from[1] == row || to[1] == row)
        && !(from[1] == row && to[1] == row)
}

pub(super) fn touches_rectangle(
    from: [i32; 2],
    to: [i32; 2],
    [col_min, col_max]: [i32; 2],
    [row_min, row_max]: [i32; 2],
) -> bool {
    let min_col = from[0].min(to[0]);
    let max_col = from[0].max(to[0]);
    let min_row = from[1].min(to[1]);
    let max_row = from[1].max(to[1]);

    if from[1] == to[1] {
        let row = from[1];
        return (row_min..=row_max).contains(&row) && (max_col == col_min || min_col == col_max);
    }

    if from[0] == to[0] {
        let col = from[0];
        return (col_min..=col_max).contains(&col) && (max_row == row_min || min_row == row_max);
    }

    false
}
