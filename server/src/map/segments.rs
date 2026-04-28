use super::edges::{has_horizontal_edge, has_vertical_edge};
use crate::{constants::WALL_OVERLAP, resources::EdgeGrid};
use common::{constants::*, protocol::Floor};

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct HorizontalSegment {
    pub x1: f32,
    pub x2: f32,
    pub z: f32,
}

impl HorizontalSegment {
    #[must_use]
    pub fn overlap(self, other: Self) -> Option<Self> {
        let x1 = self.x1.max(other.x1);
        let x2 = self.x2.min(other.x2);
        (x2 > x1).then_some(Self { x1, x2, z: self.z })
    }

    #[must_use]
    pub fn floor_strip(self, y: f32, thickness: f32, level: u8) -> Floor {
        let half_width = WALL_THICKNESS / 2.0;
        Floor {
            x1: self.x1,
            z1: self.z - half_width,
            x2: self.x2,
            z2: self.z + half_width,
            y,
            thickness,
            level,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct VerticalSegment {
    pub x: f32,
    pub z1: f32,
    pub z2: f32,
}

impl VerticalSegment {
    #[must_use]
    pub fn overlap(self, other: Self) -> Option<Self> {
        let z1 = self.z1.max(other.z1);
        let z2 = self.z2.min(other.z2);
        (z2 > z1).then_some(Self { x: self.x, z1, z2 })
    }

    #[must_use]
    pub fn floor_strip(self, y: f32, thickness: f32, level: u8) -> Floor {
        let half_width = WALL_THICKNESS / 2.0;
        Floor {
            x1: self.x - half_width,
            z1: self.z1,
            x2: self.x + half_width,
            z2: self.z2,
            y,
            thickness,
            level,
        }
    }
}

#[must_use]
pub(super) fn grid_x(col: i32) -> f32 {
    (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
}

#[must_use]
pub(super) fn grid_z(row: i32) -> f32 {
    (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0))
}

#[must_use]
pub(super) fn horizontal_wall_segment(
    edge_grid: &EdgeGrid,
    row: i32,
    col: i32,
    grid_cols: i32,
    grid_rows: i32,
) -> HorizontalSegment {
    let has_left = col > 0 && has_horizontal_edge(edge_grid, row, col - 1);
    let has_right = col < grid_cols - 1 && has_horizontal_edge(edge_grid, row, col + 1);

    let left_vert_top = row > 0 && has_vertical_edge(edge_grid, row - 1, col);
    let left_vert_bottom = row < grid_rows && has_vertical_edge(edge_grid, row, col);
    let left_vert_through = left_vert_top && left_vert_bottom;

    let right_vert_top = row > 0 && has_vertical_edge(edge_grid, row - 1, col + 1);
    let right_vert_bottom = row < grid_rows && has_vertical_edge(edge_grid, row, col + 1);
    let right_vert_through = right_vert_top && right_vert_bottom;

    let x1 = grid_x(col)
        + if WALL_OVERLAP {
            -WALL_THICKNESS / 2.0
        } else if left_vert_through && !has_left {
            WALL_THICKNESS / 2.0
        } else if !has_left {
            -WALL_THICKNESS / 2.0
        } else {
            0.0
        };
    let x2 = grid_x(col + 1)
        + if WALL_OVERLAP {
            WALL_THICKNESS / 2.0
        } else if right_vert_through && !has_right {
            -WALL_THICKNESS / 2.0
        } else if !has_right {
            WALL_THICKNESS / 2.0
        } else {
            0.0
        };

    HorizontalSegment { x1, x2, z: grid_z(row) }
}

#[must_use]
pub(super) fn vertical_wall_segment(
    edge_grid: &EdgeGrid,
    row: i32,
    col: i32,
    grid_cols: i32,
    grid_rows: i32,
) -> VerticalSegment {
    let has_top = row > 0 && has_vertical_edge(edge_grid, row - 1, col);
    let has_bottom = row < grid_rows - 1 && has_vertical_edge(edge_grid, row + 1, col);
    let (has_perp_top, has_perp_bottom) = has_perpendicular_horizontal_walls(edge_grid, row, col, grid_cols, grid_rows);

    let z1 = grid_z(row)
        + if has_perp_top && !has_top {
            WALL_THICKNESS / 2.0
        } else if !has_top && !has_perp_top {
            -WALL_THICKNESS / 2.0
        } else {
            0.0
        };
    let z2 = grid_z(row + 1)
        + if has_perp_bottom && !has_bottom {
            -WALL_THICKNESS / 2.0
        } else if !has_bottom && !has_perp_bottom {
            WALL_THICKNESS / 2.0
        } else {
            0.0
        };

    VerticalSegment { x: grid_x(col), z1, z2 }
}

#[inline]
fn has_perpendicular_horizontal_walls(
    edge_grid: &EdgeGrid,
    row: i32,
    col: i32,
    grid_cols: i32,
    grid_rows: i32,
) -> (bool, bool) {
    let has_perp_top = row > 0
        && ((col < grid_cols && has_horizontal_edge(edge_grid, row, col))
            || (col > 0 && has_horizontal_edge(edge_grid, row, col - 1)));

    let has_perp_bottom = row < grid_rows
        && ((col < grid_cols && has_horizontal_edge(edge_grid, row + 1, col))
            || (col > 0 && has_horizontal_edge(edge_grid, row + 1, col - 1)));

    (has_perp_top, has_perp_bottom)
}
