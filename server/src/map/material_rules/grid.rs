use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map_geometry::MapGeometry,
    protocol::{Floor, Ramp, Wall},
};

pub(super) fn floor_cells(geometry: &MapGeometry, floor: &Floor) -> Vec<(i32, i32)> {
    let min_x = floor.x1.min(floor.x2);
    let max_x = floor.x1.max(floor.x2);
    let min_z = floor.z1.min(floor.z2);
    let max_z = floor.z1.max(floor.z2);

    let min_col = first_cell_center_at_or_after(min_x, geometry.width());
    let max_col = last_cell_center_at_or_before(max_x, geometry.width());
    let min_row = first_cell_center_at_or_after(min_z, geometry.depth());
    let max_row = last_cell_center_at_or_before(max_z, geometry.depth());

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

pub(super) fn ramp_cells(geometry: &MapGeometry, ramp: &Ramp) -> Vec<(i32, i32)> {
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    let min_col = first_cell_center_at_or_after(min_x, geometry.width());
    let max_col = last_cell_center_at_or_before(max_x, geometry.width());
    let min_row = first_cell_center_at_or_after(min_z, geometry.depth());
    let max_row = last_cell_center_at_or_before(max_z, geometry.depth());

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

pub(super) fn wall_edges(geometry: &MapGeometry, wall: &Wall) -> Vec<([i32; 2], [i32; 2])> {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    if dx >= dz {
        let row = geometry.world_z_to_grid_row(f32::midpoint(wall.z1, wall.z2));
        let min_col = first_edge_midpoint_at_or_after(wall.x1.min(wall.x2), geometry.width());
        let max_col = last_edge_midpoint_at_or_before(wall.x1.max(wall.x2), geometry.width());
        if min_col > max_col {
            return Vec::new();
        }
        return (min_col..=max_col).map(|col| ([col, row], [col + 1, row])).collect();
    }

    let col = geometry.world_x_to_grid_col(f32::midpoint(wall.x1, wall.x2));
    let min_row = first_edge_midpoint_at_or_after(wall.z1.min(wall.z2), geometry.depth());
    let max_row = last_edge_midpoint_at_or_before(wall.z1.max(wall.z2), geometry.depth());
    if min_row > max_row {
        return Vec::new();
    }
    (min_row..=max_row).map(|row| ([col, row], [col, row + 1])).collect()
}

fn first_cell_center_at_or_after(world: f32, map_size: f32) -> i32 {
    ((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).ceil() as i32
}

fn last_cell_center_at_or_before(world: f32, map_size: f32) -> i32 {
    ((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).floor() as i32
}

fn first_edge_midpoint_at_or_after(world: f32, map_size: f32) -> i32 {
    first_cell_center_at_or_after(world, map_size)
}

fn last_edge_midpoint_at_or_before(world: f32, map_size: f32) -> i32 {
    last_cell_center_at_or_before(world, map_size)
}

pub(super) fn ramp_lower_level(ramp: &Ramp) -> u8 {
    let lower_y = ramp.y1.min(ramp.y2);
    (lower_y / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8
}
