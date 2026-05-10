use super::{
    mask::Mask,
    material_rules::MaterialRules,
    segments::{grid_x, grid_z, horizontal_wall_segment, vertical_wall_segment},
};
use crate::constants::FLOOR_OVERLAP;
use crate::resources::EdgeGrid;
use common::{constants::*, face_materials::FaceMaterials, map_geometry::MapGeometry, protocol::Floor};

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

// Merge adjacent floors at the same level into larger rectangles. Compares
// only the faces that remain visible after merging: when joining along x,
// top/bottom/north/south must match (east/west become end caps); when joining
// along z, top/bottom/east/west must match (north/south become end caps).
//
// The merged rectangle inherits its long-side / top-bottom materials from
// the matched set, and takes its outer end caps from the source rectangles
// at each extremity.
pub fn merge_floors(floors: Vec<Floor>, assets: &MaterialRules) -> (Vec<Floor>, Vec<FaceMaterials>) {
    let paired: Vec<(Floor, FaceMaterials)> = floors
        .into_iter()
        .map(|f| {
            let m = assets.materials_for_floor(&f);
            (f, m)
        })
        .collect();
    merge_floors_with_materials(paired)
}

// Inner merge driver — operates on `(Floor, FaceMaterials)` pairs so tests can
// supply face materials directly without needing a full `MaterialRules`.
fn merge_floors_with_materials(paired: Vec<(Floor, FaceMaterials)>) -> (Vec<Floor>, Vec<FaceMaterials>) {
    let mut paired: Vec<(Floor, FaceMaterials)> = paired
        .into_iter()
        .map(|(mut f, m)| {
            if f.x1 > f.x2 {
                std::mem::swap(&mut f.x1, &mut f.x2);
            }
            if f.z1 > f.z2 {
                std::mem::swap(&mut f.z1, &mut f.z2);
            }
            (f, m)
        })
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        let mut used = vec![false; paired.len()];
        let mut out: Vec<(Floor, FaceMaterials)> = Vec::new();

        for i in 0..paired.len() {
            if used[i] {
                continue;
            }
            let (mut acc, mut acc_materials) = paired[i].clone();
            used[i] = true;

            let mut merged_this_round = true;
            while merged_this_round {
                merged_this_round = false;
                for j in 0..paired.len() {
                    if used[j] {
                        continue;
                    }
                    let (b, b_materials) = &paired[j];
                    if (acc.thickness - b.thickness).abs() >= MERGE_EPS
                        || acc.level != b.level
                        || (acc.y - b.y).abs() >= MERGE_EPS
                    {
                        continue;
                    }

                    let same_z_span = (acc.z1 - b.z1).abs() < MERGE_EPS && (acc.z2 - b.z2).abs() < MERGE_EPS;
                    let adjacent_x = (acc.x2 - b.x1).abs() < MERGE_EPS || (b.x2 - acc.x1).abs() < MERGE_EPS;
                    let same_x_span = (acc.x1 - b.x1).abs() < MERGE_EPS && (acc.x2 - b.x2).abs() < MERGE_EPS;
                    let adjacent_z = (acc.z2 - b.z1).abs() < MERGE_EPS || (b.z2 - acc.z1).abs() < MERGE_EPS;

                    if same_z_span && adjacent_x && x_merge_visible_match(&acc_materials, b_materials) {
                        if b.x1 < acc.x1 {
                            // b is west of acc — its west face becomes the merged west cap.
                            acc.x1 = b.x1;
                            acc_materials.west = b_materials.west.clone();
                        }
                        if b.x2 > acc.x2 {
                            acc.x2 = b.x2;
                            acc_materials.east = b_materials.east.clone();
                        }
                        used[j] = true;
                        merged_this_round = true;
                        changed = true;
                    } else if same_x_span && adjacent_z && z_merge_visible_match(&acc_materials, b_materials) {
                        if b.z1 < acc.z1 {
                            acc.z1 = b.z1;
                            acc_materials.north = b_materials.north.clone();
                        }
                        if b.z2 > acc.z2 {
                            acc.z2 = b.z2;
                            acc_materials.south = b_materials.south.clone();
                        }
                        used[j] = true;
                        merged_this_round = true;
                        changed = true;
                    }
                }
            }
            out.push((acc, acc_materials));
        }

        paired = out;
    }

    let mut floors_out = Vec::with_capacity(paired.len());
    let mut materials_out = Vec::with_capacity(paired.len());
    for (f, m) in paired {
        floors_out.push(f);
        materials_out.push(m);
    }
    (floors_out, materials_out)
}

// Visible faces when merging in the x direction (rectangles abut on east/west).
fn x_merge_visible_match(a: &FaceMaterials, b: &FaceMaterials) -> bool {
    a.top == b.top && a.bottom == b.bottom && a.north == b.north && a.south == b.south
}

// Visible faces when merging in the z direction (rectangles abut on north/south).
fn z_merge_visible_match(a: &FaceMaterials, b: &FaceMaterials) -> bool {
    a.top == b.top && a.bottom == b.bottom && a.east == b.east && a.west == b.west
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

    fn rect(x1: f32, x2: f32, z1: f32, z2: f32) -> Floor {
        Floor {
            x1,
            x2,
            z1,
            z2,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
        }
    }

    #[test]
    fn floors_merge_across_x_when_only_hidden_caps_differ() {
        let left = (rect(0.0, 1.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "n", "s", "INNER", "outer_W"));
        let right = (rect(1.0, 2.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "n", "s", "outer_E", "INNER"));

        let (floors, materials) = merge_floors_with_materials(vec![left, right]);

        assert_eq!(floors.len(), 1);
        assert!((floors[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((floors[0].x2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].west, "outer_W");
        assert_eq!(materials[0].east, "outer_E");
    }

    #[test]
    fn floors_do_not_merge_across_x_when_visible_face_differs() {
        let left = (rect(0.0, 1.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "n", "s", "e", "w"));
        // North is a visible long face when merging in x.
        let right = (rect(1.0, 2.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "DIFFERENT", "s", "e", "w"));

        let (floors, _) = merge_floors_with_materials(vec![left, right]);

        assert_eq!(floors.len(), 2);
    }

    #[test]
    fn floors_merge_across_z_when_only_hidden_caps_differ() {
        let north_rect = (rect(0.0, 1.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "outer_N", "INNER", "e", "w"));
        let south_rect = (rect(0.0, 1.0, 1.0, 2.0), FaceMaterials::from_six("t", "b", "INNER", "outer_S", "e", "w"));

        let (floors, materials) = merge_floors_with_materials(vec![north_rect, south_rect]);

        assert_eq!(floors.len(), 1);
        assert!((floors[0].z1 - 0.0).abs() < MERGE_EPS);
        assert!((floors[0].z2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].north, "outer_N");
        assert_eq!(materials[0].south, "outer_S");
    }

    #[test]
    fn floors_chain_x_then_z_with_correct_outer_caps() {
        // A,B abut along x; AB then abuts the row below (C,D) along z.
        // Each cell has distinct east/west AND distinct north/south, but the
        // north of A == north of B (visible during x merge), and the
        // east/west of (AB) need to match east/west of (CD) for the z merge.
        let a = (rect(0.0, 1.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "n_top", "s_inner", "e_inner", "outer_W"));
        let b = (rect(1.0, 2.0, 0.0, 1.0), FaceMaterials::from_six("t", "b", "n_top", "s_inner", "outer_E", "e_inner"));
        let c = (rect(0.0, 1.0, 1.0, 2.0), FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "e_inner", "outer_W"));
        let d = (rect(1.0, 2.0, 1.0, 2.0), FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "outer_E", "e_inner"));

        let (floors, materials) = merge_floors_with_materials(vec![a, b, c, d]);

        assert_eq!(floors.len(), 1);
        assert!((floors[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((floors[0].x2 - 2.0).abs() < MERGE_EPS);
        assert!((floors[0].z1 - 0.0).abs() < MERGE_EPS);
        assert!((floors[0].z2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].west, "outer_W");
        assert_eq!(materials[0].east, "outer_E");
        assert_eq!(materials[0].north, "n_top");
        assert_eq!(materials[0].south, "s_bot");
    }
}
