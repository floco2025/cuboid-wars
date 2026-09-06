use std::collections::HashMap;

use common::{
    map::MapGeometry,
    protocol::{FaceMaterials, Floor, Ramp, Wall},
};

use super::{
    grid::{floor_cells, ramp_cells, ramp_lower_level, wall_edges},
    loading::wall_edge_key,
};

// How close a coordinate must be to a grid line to count as "on" it. Smaller
// than the corner-filler offset (`WALL_HALF_THICKNESS`) so the two sides of a
// thin strip are always distinguishable.
const GRID_LINE_EPS: f32 = 0.05;

#[derive(Debug, Clone, Default)]
pub(super) struct SegmentMaterials {
    // Keyed by (level, col, row).
    pub(super) floors: HashMap<(u8, i32, i32), FaceMaterials>,
    // Keyed by (level, normalized edge endpoints).
    pub(super) walls: HashMap<(u8, [i32; 2], [i32; 2]), FaceMaterials>,
    // Keyed by (lower_level, col, row) for each cell in the ramp footprint.
    pub(super) ramps: HashMap<(u8, i32, i32), FaceMaterials>,
}

#[derive(Debug, Clone)]
pub struct MaterialRules {
    pub(super) geometry: MapGeometry,
    pub(super) segments: SegmentMaterials,
}

impl MaterialRules {
    #[must_use]
    pub fn materials_for_floor(&self, floor: &Floor) -> FaceMaterials {
        for (col, row) in floor_cells(&self.geometry, floor) {
            if let Some(materials) = self.segments.floors.get(&(floor.level, col, row)) {
                return materials.clone();
            }
        }
        // Stacked-wall trims and corner-filler strips don't span a full floor
        // cell, so the loop above misses them. Lookup order:
        //   1. Floor at the segment's world midpoint.
        //   2. Corner filler: a thin strip hanging off one edge of a real
        //      floor cell. Detect it by shape and use *that* cell directly,
        //      so the strip inherits its long-side neighbour's material
        //      rather than a diagonal cell's.
        //   3. Any cardinal neighbour of the midpoint that has a floor.
        //   4. Adjacent wall on this level.
        let mid_col = self.geometry.cell_col_containing_x(f32::midpoint(floor.x1, floor.x2));
        let mid_row = self.geometry.cell_row_containing_z(f32::midpoint(floor.z1, floor.z2));
        if let Some(materials) = self.segments.floors.get(&(floor.level, mid_col, mid_row)) {
            return materials.clone();
        }
        if let Some(materials) = self.corner_filler_originating_cell_materials(floor) {
            return materials;
        }
        if let Some(materials) = self.adjacent_floor_materials(floor.level, mid_col, mid_row) {
            return materials;
        }
        if let Some(materials) = self.adjacent_wall_materials(floor.level, mid_col, mid_row) {
            return materials;
        }
        self.first_floor_material_on_level(floor.level)
            .unwrap_or_else(missing_materials)
    }

    // A corner-filler strip from `emit_floor_tier` is thin in one axis
    // (~HALF_WALL_THICKNESS) and has one boundary exactly on a grid line
    // (the side attached to the originating cell) and the opposite boundary
    // offset by `pad` (the side hanging into the cell beyond). Detect that
    // shape and return the originating cell's material, so the filler reads
    // visually as an extension of its long-side neighbour rather than of an
    // unrelated diagonal cell.
    fn corner_filler_originating_cell_materials(&self, floor: &Floor) -> Option<FaceMaterials> {
        let z_extent = (floor.z2 - floor.z1).abs();
        let x_extent = (floor.x2 - floor.x1).abs();
        let half_cell = self.geometry.cell_size() / 2.0;
        if z_extent >= half_cell && x_extent >= half_cell {
            return None;
        }
        let mid_col = self.geometry.cell_col_containing_x(f32::midpoint(floor.x1, floor.x2));
        let mid_row = self.geometry.cell_row_containing_z(f32::midpoint(floor.z1, floor.z2));
        // Thin in z: originating cell sits on whichever z side is on a grid
        // line. Same logic mirrored for x (no x-thin fillers exist today,
        // but the symmetry is cheap to keep).
        let (origin_col, origin_row) = if z_extent < x_extent {
            let max_z = floor.z1.max(floor.z2);
            let min_z = floor.z1.min(floor.z2);
            if self.is_on_grid_z(max_z) && !self.is_on_grid_z(min_z) {
                (mid_col, self.geometry.cell_row_containing_z(max_z + GRID_LINE_EPS))
            } else if self.is_on_grid_z(min_z) && !self.is_on_grid_z(max_z) {
                (mid_col, self.geometry.cell_row_containing_z(min_z - GRID_LINE_EPS))
            } else {
                return None;
            }
        } else {
            let max_x = floor.x1.max(floor.x2);
            let min_x = floor.x1.min(floor.x2);
            if self.is_on_grid_x(max_x) && !self.is_on_grid_x(min_x) {
                (self.geometry.cell_col_containing_x(max_x + GRID_LINE_EPS), mid_row)
            } else if self.is_on_grid_x(min_x) && !self.is_on_grid_x(max_x) {
                (self.geometry.cell_col_containing_x(min_x - GRID_LINE_EPS), mid_row)
            } else {
                return None;
            }
        };
        self.segments
            .floors
            .get(&(floor.level, origin_col, origin_row))
            .cloned()
    }

    fn is_on_grid_z(&self, z: f32) -> bool {
        let nearest = self.geometry.nearest_grid_row_to_z(z);
        (z - self.geometry.cell_to_world_z(nearest)).abs() < GRID_LINE_EPS
    }

    fn is_on_grid_x(&self, x: f32) -> bool {
        let nearest = self.geometry.nearest_grid_col_to_x(x);
        (x - self.geometry.cell_to_world_x(nearest)).abs() < GRID_LINE_EPS
    }

    fn adjacent_floor_materials(&self, level: u8, col: i32, row: i32) -> Option<FaceMaterials> {
        for (c, r) in [(col - 1, row), (col + 1, row), (col, row - 1), (col, row + 1)] {
            if let Some(m) = self.segments.floors.get(&(level, c, r)) {
                return Some(m.clone());
            }
        }
        None
    }

    fn adjacent_wall_materials(&self, level: u8, col: i32, row: i32) -> Option<FaceMaterials> {
        let candidates = [
            wall_edge_key([col, row], [col + 1, row]),
            wall_edge_key([col, row + 1], [col + 1, row + 1]),
            wall_edge_key([col, row], [col, row + 1]),
            wall_edge_key([col + 1, row], [col + 1, row + 1]),
        ];
        for (a, b) in candidates {
            if let Some(m) = self.segments.walls.get(&(level, a, b)) {
                return Some(m.clone());
            }
        }
        None
    }

    fn first_floor_material_on_level(&self, level: u8) -> Option<FaceMaterials> {
        self.segments
            .floors
            .iter()
            .find(|((l, _, _), _)| *l == level)
            .map(|(_, m)| m.clone())
    }

    #[must_use]
    pub fn materials_for_wall(&self, wall: &Wall) -> FaceMaterials {
        for (from, to) in wall_edges(&self.geometry, wall) {
            let (a, b) = wall_edge_key(from, to);
            if let Some(materials) = self.segments.walls.get(&(wall.level, a, b)) {
                return materials.clone();
            }
        }
        let from = [
            self.geometry.nearest_grid_col_to_x(wall.x1),
            self.geometry.nearest_grid_row_to_z(wall.z1),
        ];
        let to = [
            self.geometry.nearest_grid_col_to_x(wall.x2),
            self.geometry.nearest_grid_row_to_z(wall.z2),
        ];
        let (a, b) = wall_edge_key(from, to);
        if let Some(materials) = self.segments.walls.get(&(wall.level, a, b)) {
            return materials.clone();
        }
        self.first_wall_material_on_level(wall.level)
            .unwrap_or_else(missing_materials)
    }

    fn first_wall_material_on_level(&self, level: u8) -> Option<FaceMaterials> {
        self.segments
            .walls
            .iter()
            .find(|((l, _, _), _)| *l == level)
            .map(|(_, m)| m.clone())
    }

    #[must_use]
    pub fn materials_for_ramp_top(&self, ramp: &Ramp) -> FaceMaterials {
        let lower_level = ramp_lower_level(&self.geometry, ramp);
        for (col, row) in ramp_cells(&self.geometry, ramp) {
            if let Some(materials) = self.segments.ramps.get(&(lower_level, col, row)) {
                return materials.clone();
            }
            // Fall back to floor materials at the ramp footprint cell.
            if let Some(materials) = self.segments.floors.get(&(lower_level, col, row)) {
                return materials.clone();
            }
        }
        missing_materials()
    }
}

fn missing_materials() -> FaceMaterials {
    // Returned when a query lands on a segment that wasn't in the map. The
    // mesh emission paths shouldn't hit this in normal operation; the value is
    // visually distinctive so the issue is obvious if it does.
    FaceMaterials::uniform("__missing__")
}
