use super::{
    mask::Mask,
    segments::{grid_x, grid_z, horizontal_wall_segment, vertical_wall_segment},
};
use crate::constants::FLOOR_OVERLAP;
use crate::resources::EdgeGrid;
use common::{constants::*, map_geometry::MapGeometry, material_rules::MaterialRules, protocol::Floor};

const MERGE_EPS: f32 = 0.01;
const CORNER_EPS: f32 = 0.01;

// Emit `Floor` segments for every cell in `mask` at level `level` (Y = `y`).
// Cells with a 4-connected neighbor on a side don't extend on that side
// (their slabs meet along the grid line). Cells without a 4-connected
// neighbor are extended outward by `WALL_THICKNESS / 2` so the slab covers
// where a perimeter wall would sit.
//
// When a diagonal neighbor exists on the N/S side without a 4-connected
// neighbor, the N/S extension is suppressed (it would overlap and z-fight
// with the diagonal cell's W/E extension). The remaining corner gap is
// patched by a thin `edge_filler` strip inset from the corner so it doesn't
// overlap either of the adjacent extended slabs.
//
// All levels use the same `FLOOR_THICKNESS` so a hole in any tier looks like
// a real slab edge.
#[must_use]
pub fn emit_floor_tier(mask: &Mask, geometry: &MapGeometry, level: u8, y: f32) -> Vec<Floor> {
    let grid_cols = geometry.grid_cols;
    let grid_rows = geometry.grid_rows;
    let thickness = FLOOR_THICKNESS;
    let mut floors = Vec::new();

    let in_mask = |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && mask[r as usize][c as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !mask[row as usize][col as usize] {
                continue;
            }

            let (world_x1, world_x2, world_z1, world_z2, edge_fillers) = if FLOOR_OVERLAP {
                let x1 = grid_x(geometry, col) - WALL_THICKNESS / 2.0;
                let x2 = grid_x(geometry, col + 1) + WALL_THICKNESS / 2.0;
                let z1 = grid_z(geometry, row) - WALL_THICKNESS / 2.0;
                let z2 = grid_z(geometry, row + 1) + WALL_THICKNESS / 2.0;
                (x1, x2, z1, z2, Vec::new())
            } else {
                let x1_orig = grid_x(geometry, col);
                let x2_orig = grid_x(geometry, col + 1);
                let z1_orig = grid_z(geometry, row);
                let z2_orig = grid_z(geometry, row + 1);

                let mut x1 = x1_orig;
                let mut x2 = x2_orig;
                let mut z1 = z1_orig;
                let mut z2 = z2_orig;
                let mut edge_fillers: Vec<Floor> = Vec::new();

                let neighbor_w = in_mask(row, col - 1);
                let neighbor_e = in_mask(row, col + 1);
                let neighbor_n = in_mask(row - 1, col);
                let neighbor_s = in_mask(row + 1, col);

                let neighbor_nw = in_mask(row - 1, col - 1);
                let neighbor_ne = in_mask(row - 1, col + 1);
                let neighbor_sw = in_mask(row + 1, col - 1);
                let neighbor_se = in_mask(row + 1, col + 1);

                let extend_w = !neighbor_w;
                let extend_e = !neighbor_e;
                let mut extend_n = !neighbor_n;
                let mut extend_s = !neighbor_s;

                // Diagonal suppression: skip the N/S extension when a
                // diagonal cell sits on that side. Otherwise the N/S
                // extension would overlap the diagonal cell's W/E extension.
                if neighbor_nw || neighbor_ne {
                    extend_n = false;
                }
                if neighbor_sw || neighbor_se {
                    extend_s = false;
                }

                if extend_w {
                    x1 -= WALL_THICKNESS / 2.0;
                }
                if extend_e {
                    x2 += WALL_THICKNESS / 2.0;
                }
                if extend_n {
                    z1 -= WALL_THICKNESS / 2.0;
                }
                if extend_s {
                    z2 += WALL_THICKNESS / 2.0;
                }

                // Corner-filler strips. When a diagonal neighbor suppressed
                // the N/S extension, the resulting L-shape leaves a gap on
                // the side of the cell opposite the diagonal. Add a thin
                // strip there. The inset must use the *unextended* grid-line
                // (`x1_orig`/`x2_orig`) plus `pad`, because the diagonal
                // cell's W/E extension reaches `pad` past the grid line —
                // insetting from the cell's own extended `x1`/`x2` would
                // still overlap the diagonal cell.
                let pad = (WALL_THICKNESS / 2.0) - CORNER_EPS;
                if pad > 0.0 {
                    if !extend_n && !neighbor_n && (neighbor_nw || neighbor_ne) {
                        let fx1 = if neighbor_nw { x1_orig + pad } else { x1 };
                        let fx2 = if neighbor_ne { x2_orig - pad } else { x2 };
                        if fx2 > fx1 {
                            edge_fillers.push(Floor {
                                x1: fx1,
                                z1: z1_orig - pad,
                                x2: fx2,
                                z2: z1_orig,
                                y,
                                thickness,
                                level,
                            });
                        }
                    }
                    if !extend_s && !neighbor_s && (neighbor_sw || neighbor_se) {
                        let fx1 = if neighbor_sw { x1_orig + pad } else { x1 };
                        let fx2 = if neighbor_se { x2_orig - pad } else { x2 };
                        if fx2 > fx1 {
                            edge_fillers.push(Floor {
                                x1: fx1,
                                z1: z2_orig,
                                x2: fx2,
                                z2: z2_orig + pad,
                                y,
                                thickness,
                                level,
                            });
                        }
                    }
                }

                (x1, x2, z1, z2, edge_fillers)
            };

            floors.push(Floor {
                x1: world_x1,
                z1: world_z1,
                x2: world_x2,
                z2: world_z2,
                y,
                thickness,
                level,
            });

            floors.extend(edge_fillers);
        }
    }

    floors
}

#[must_use]
pub fn emit_stacked_wall_trim(
    lower_edges: &EdgeGrid,
    upper_edges: &EdgeGrid,
    upper_mask: &Mask,
    geometry: &MapGeometry,
    level: u8,
    y: f32,
) -> Vec<Floor> {
    let grid_cols = geometry.grid_cols;
    let grid_rows = geometry.grid_rows;
    let mut floors = Vec::new();
    let in_upper_mask =
        |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && upper_mask[r as usize][c as usize];

    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !lower_edges.horizontal[row as usize][col as usize]
                || !upper_edges.horizontal[row as usize][col as usize]
            {
                continue;
            }
            if in_upper_mask(row - 1, col) || in_upper_mask(row, col) {
                continue;
            }
            let lower_segment = horizontal_wall_segment(lower_edges, row, col, geometry);
            let upper_segment = horizontal_wall_segment(upper_edges, row, col, geometry);
            if let Some(segment) = lower_segment.overlap(upper_segment) {
                floors.push(segment.floor_strip(y, FLOOR_THICKNESS, level));
            }
        }
    }

    for row in 0..grid_rows {
        for col in 0..=grid_cols {
            if !lower_edges.vertical[row as usize][col as usize] || !upper_edges.vertical[row as usize][col as usize] {
                continue;
            }
            if in_upper_mask(row, col - 1) || in_upper_mask(row, col) {
                continue;
            }
            let lower_segment = vertical_wall_segment(lower_edges, row, col, geometry);
            let upper_segment = vertical_wall_segment(upper_edges, row, col, geometry);
            if let Some(segment) = lower_segment.overlap(upper_segment) {
                floors.push(segment.floor_strip(y, FLOOR_THICKNESS, level));
            }
        }
    }

    floors
}

// Merge adjacent floors at the same level into larger segments.
pub fn merge_floors(mut floors: Vec<Floor>, assets: &MaterialRules) -> Vec<Floor> {
    for r in &mut floors {
        if r.x1 > r.x2 {
            std::mem::swap(&mut r.x1, &mut r.x2);
        }
        if r.z1 > r.z2 {
            std::mem::swap(&mut r.z1, &mut r.z2);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        let mut used = vec![false; floors.len()];
        let mut out: Vec<Floor> = Vec::new();

        for i in 0..floors.len() {
            if used[i] {
                continue;
            }
            let mut acc = floors[i];
            used[i] = true;

            let mut merged_this_round = true;
            while merged_this_round {
                merged_this_round = false;
                for j in 0..floors.len() {
                    if used[j] {
                        continue;
                    }
                    let b = floors[j];
                    let same_thickness = (acc.thickness - b.thickness).abs() < MERGE_EPS;
                    let same_level = acc.level == b.level;
                    let same_y = (acc.y - b.y).abs() < MERGE_EPS;
                    let same_material = assets.materials_for_floor(&acc) == assets.materials_for_floor(&b);
                    if !same_thickness || !same_level || !same_y || !same_material {
                        continue;
                    }

                    let same_z_span = (acc.z1 - b.z1).abs() < MERGE_EPS && (acc.z2 - b.z2).abs() < MERGE_EPS;
                    let adjacent_x = (acc.x2 - b.x1).abs() < MERGE_EPS || (b.x2 - acc.x1).abs() < MERGE_EPS;

                    let same_x_span = (acc.x1 - b.x1).abs() < MERGE_EPS && (acc.x2 - b.x2).abs() < MERGE_EPS;
                    let adjacent_z = (acc.z2 - b.z1).abs() < MERGE_EPS || (b.z2 - acc.z1).abs() < MERGE_EPS;

                    if (same_z_span && adjacent_x) || (same_x_span && adjacent_z) {
                        acc.x1 = acc.x1.min(b.x1);
                        acc.x2 = acc.x2.max(b.x2);
                        acc.z1 = acc.z1.min(b.z1);
                        acc.z2 = acc.z2.max(b.z2);
                        used[j] = true;
                        merged_this_round = true;
                        changed = true;
                    }
                }
            }
            out.push(acc);
        }

        floors = out;
    }

    floors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_mask(cols: usize, rows: usize) -> Mask {
        vec![vec![false; cols]; rows]
    }

    #[test]
    fn stacked_horizontal_wall_emits_physical_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        let half_d = geometry.depth() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w - WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].z1, -half_d + GRID_CELL_SIZE - WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].z2, -half_d + GRID_CELL_SIZE + WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].y, LEVEL_HEIGHT);
        assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
        assert_eq!(floors[0].level, 1);
    }

    #[test]
    fn stacked_vertical_wall_emits_physical_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.vertical[0][1] = true;
        upper_edges.vertical[0][1] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        let half_d = geometry.depth() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w + GRID_CELL_SIZE - WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].z1, -half_d - WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].z2, -half_d + GRID_CELL_SIZE + WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].y, LEVEL_HEIGHT);
        assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
        assert_eq!(floors[0].level, 1);
    }

    #[test]
    fn unstacked_wall_does_not_emit_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.horizontal[1][0] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        assert!(floors.is_empty());
    }

    #[test]
    fn stacked_wall_next_to_upper_floor_does_not_duplicate_trim() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = vec![vec![true]];
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;
        lower_edges.vertical[0][1] = true;
        upper_edges.vertical[0][1] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        assert!(floors.is_empty());
    }

    #[test]
    fn stacked_trim_uses_overlapping_wall_endpoint_rules() {
        let mut lower_edges = EdgeGrid::new(1, 2);
        let mut upper_edges = EdgeGrid::new(1, 2);
        let upper_mask = empty_mask(1, 2);
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;
        upper_edges.vertical[0][0] = true;
        upper_edges.vertical[1][0] = true;

        let geometry = MapGeometry::new(1, 2);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w + WALL_THICKNESS / 2.0);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_THICKNESS / 2.0);
    }
}
