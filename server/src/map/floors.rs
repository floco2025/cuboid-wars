use super::mask::Mask;
use crate::constants::FLOOR_OVERLAP;
use common::{constants::*, protocol::Floor};

const MERGE_EPS: f32 = 0.01;
const CORNER_EPS: f32 = 0.01;

// Emit `Floor` segments for every cell in `mask` at level `level` (Y = `y`).
// Cells that have neighbors in the same mask get their slab edges nudged so
// adjacent slabs meet without overlap; cells with no neighbor on a side are
// extended outward to cover the wall thickness. All levels use the same
// `FLOOR_THICKNESS` so a hole in any tier looks like a real edge.
#[must_use]
pub fn emit_floor_tier(mask: &Mask, grid_cols: i32, grid_rows: i32, level: u8, y: f32) -> Vec<Floor> {
    let thickness = FLOOR_THICKNESS;
    let mut floors = Vec::new();

    let in_mask = |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && mask[r as usize][c as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !mask[row as usize][col as usize] {
                continue;
            }

            let (world_x1, world_x2, world_z1, world_z2, edge_fillers) = if FLOOR_OVERLAP {
                let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) - WALL_THICKNESS / 2.0;
                let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) + WALL_THICKNESS / 2.0;
                let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) - WALL_THICKNESS / 2.0;
                let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) + WALL_THICKNESS / 2.0;
                (x1, x2, z1, z2, Vec::new())
            } else {
                let mut x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let mut x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let mut z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                let mut z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
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

                let pad = (WALL_THICKNESS / 2.0) - CORNER_EPS;
                if pad > 0.0 {
                    if !extend_n && !neighbor_n && (neighbor_nw || neighbor_ne) {
                        let fx1 = if neighbor_nw { x1 + pad } else { x1 };
                        let fx2 = if neighbor_ne { x2 - pad } else { x2 };
                        if fx2 > fx1 {
                            edge_fillers.push(Floor {
                                x1: fx1,
                                z1: z1 - pad,
                                x2: fx2,
                                z2: z1,
                                y,
                                thickness,
                                level,
                            });
                        }
                    }
                    if !extend_s && !neighbor_s && (neighbor_sw || neighbor_se) {
                        let fx1 = if neighbor_sw { x1 + pad } else { x1 };
                        let fx2 = if neighbor_se { x2 - pad } else { x2 };
                        if fx2 > fx1 {
                            edge_fillers.push(Floor {
                                x1: fx1,
                                z1: z2,
                                x2: fx2,
                                z2: z2 + pad,
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

// Merge adjacent floors at the same level into larger segments.
pub fn merge_floors(mut floors: Vec<Floor>) -> Vec<Floor> {
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
                    if !same_thickness || !same_level || !same_y {
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
