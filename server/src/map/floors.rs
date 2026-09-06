use std::collections::HashSet;

use super::{
    mask::Mask,
    material_rules::MaterialRules,
    segments::{MERGE_EPS, grid_x, grid_z},
};
use common::protocol::CarrierId;
use common::{map::MapGeometry, protocol::FaceMaterials, protocol::Floor};

// One floor cell's 8 neighbours within the level's slab mask.
struct Neighbors {
    n: bool,
    s: bool,
    e: bool,
    w: bool,
    nw: bool,
    ne: bool,
    sw: bool,
    se: bool,
}

// Emit a `Floor` cuboid for every cell in `mask` at level `level` (Y = `y`).
//
// Slab + extensions: each cell starts at its grid bounds. Sides without a
// 4-connected neighbour extend outward by half the wall thickness so the slab
// covers where a perimeter wall would sit; sides with a neighbour stop at
// the grid line (their slabs meet there). N/S extensions are additionally
// suppressed when a diagonal cell sits on that side, since the diagonal's
// W/E extension would already overlap the N/S extension.
//
// Corner fillers: when a diagonal suppresses the N/S extension, the cell is
// left L-shaped with a small gap at the corner opposite the diagonal. A
// thin strip patches that gap out to the wall face. Fillers exist only in the
// N/S direction (E/W extensions don't have a diagonal-suppression case).
//
// All tiers use the map's floor thickness so a hole in any level reads as
// a real slab edge from below.
//
// `skip_corner_filler_edges`: horizontal wall edges (`EdgeGrid.horizontal`
// index space) where the corner-filler branch must not emit. Populated from
// the high end of each z-axis ramp arriving at this level — a filler there
// would hover redundantly above where the slope already meets the upper
// floor. Only horizontal edges are tracked; no E/W counterpart is needed.
#[must_use]
pub fn emit_floor_tier(
    mask: &Mask,
    skip_corner_filler_edges: &HashSet<(i32, i32)>,
    geometry: &MapGeometry,
    level: u8,
    y: f32,
) -> Vec<Floor> {
    let grid_cols = geometry.grid_cols;
    let grid_rows = geometry.grid_rows;
    let thickness = geometry.floor_thickness();
    let wall_half_thickness = geometry.wall_half_thickness();
    let mut floors = Vec::new();

    let in_mask = |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && mask[r as usize][c as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !mask[row as usize][col as usize] {
                continue;
            }

            let x1_orig = grid_x(geometry, col);
            let x2_orig = grid_x(geometry, col + 1);
            let z1_orig = grid_z(geometry, row);
            let z2_orig = grid_z(geometry, row + 1);

            let n = Neighbors {
                w: in_mask(row, col - 1),
                e: in_mask(row, col + 1),
                n: in_mask(row - 1, col),
                s: in_mask(row + 1, col),
                nw: in_mask(row - 1, col - 1),
                ne: in_mask(row - 1, col + 1),
                sw: in_mask(row + 1, col - 1),
                se: in_mask(row + 1, col + 1),
            };

            let extend_w = !n.w;
            let extend_e = !n.e;
            // Diagonal suppression: skip the N/S extension when a diagonal
            // cell sits on that side. Otherwise the N/S extension would
            // overlap the diagonal cell's W/E extension.
            let extend_n = !n.n && !n.nw && !n.ne;
            let extend_s = !n.s && !n.sw && !n.se;

            let x1 = if extend_w {
                x1_orig - wall_half_thickness
            } else {
                x1_orig
            };
            let x2 = if extend_e {
                x2_orig + wall_half_thickness
            } else {
                x2_orig
            };
            let z1 = if extend_n {
                z1_orig - wall_half_thickness
            } else {
                z1_orig
            };
            let z2 = if extend_s {
                z2_orig + wall_half_thickness
            } else {
                z2_orig
            };

            floors.push(Floor {
                x1,
                z1,
                x2,
                z2,
                y,
                thickness,
                level,
                carrier: CarrierId::WORLD,
            });

            // Corner fillers. Use the *unextended* grid line
            // (`x1_orig`/`x2_orig`) plus `pad`, because the diagonal cell's
            // W/E extension also reaches `pad` past the grid line. The N
            // filler lands at horizontal[row][col]; the S filler at
            // horizontal[row+1][col]. Skip emission if that edge is in the
            // skip set (a ramp's high-end edge).
            let pad = wall_half_thickness;
            if pad <= 0.0 {
                continue;
            }
            if !extend_n && !n.n && (n.nw || n.ne) && !skip_corner_filler_edges.contains(&(row, col)) {
                let fx1 = if n.nw { x1_orig + pad } else { x1 };
                let fx2 = if n.ne { x2_orig - pad } else { x2 };
                if fx2 > fx1 {
                    floors.push(Floor {
                        x1: fx1,
                        z1: z1_orig - pad,
                        x2: fx2,
                        z2: z1_orig,
                        y,
                        thickness,
                        level,
                        carrier: CarrierId::WORLD,
                    });
                }
            }
            if !extend_s && !n.s && (n.sw || n.se) && !skip_corner_filler_edges.contains(&(row + 1, col)) {
                let fx1 = if n.sw { x1_orig + pad } else { x1 };
                let fx2 = if n.se { x2_orig - pad } else { x2 };
                if fx2 > fx1 {
                    floors.push(Floor {
                        x1: fx1,
                        z1: z2_orig,
                        x2: fx2,
                        z2: z2_orig + pad,
                        y,
                        thickness,
                        level,
                        carrier: CarrierId::WORLD,
                    });
                }
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

                    if try_merge_x(&mut acc, &mut acc_materials, b, b_materials)
                        || try_merge_z(&mut acc, &mut acc_materials, b, b_materials)
                    {
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

// Attempt to grow `acc` along the x axis by absorbing `b`. `b` qualifies when
// the two rectangles share their z span exactly and abut (or overlap) along
// x, and their visible faces in the x-merge direction (top/bottom/north/south)
// match. On success, `acc`'s x extent grows and its outer east/west caps
// follow `b`'s east/west when `b` extends past the current span.
fn try_merge_x(acc: &mut Floor, acc_m: &mut FaceMaterials, b: &Floor, b_m: &FaceMaterials) -> bool {
    let same_z_span = (acc.z1 - b.z1).abs() < MERGE_EPS && (acc.z2 - b.z2).abs() < MERGE_EPS;
    let adjacent_x = (acc.x2 - b.x1).abs() < MERGE_EPS || (b.x2 - acc.x1).abs() < MERGE_EPS;
    let faces_match =
        acc_m.top == b_m.top && acc_m.bottom == b_m.bottom && acc_m.north == b_m.north && acc_m.south == b_m.south;
    if !(same_z_span && adjacent_x && faces_match) {
        return false;
    }
    if b.x1 < acc.x1 {
        acc.x1 = b.x1;
        acc_m.west = b_m.west.clone();
    }
    if b.x2 > acc.x2 {
        acc.x2 = b.x2;
        acc_m.east = b_m.east.clone();
    }
    true
}

// z-axis counterpart to `try_merge_x`. Same shape, with x and z swapped and
// east/west replaced by south/north as the visible long faces.
fn try_merge_z(acc: &mut Floor, acc_m: &mut FaceMaterials, b: &Floor, b_m: &FaceMaterials) -> bool {
    let same_x_span = (acc.x1 - b.x1).abs() < MERGE_EPS && (acc.x2 - b.x2).abs() < MERGE_EPS;
    let adjacent_z = (acc.z2 - b.z1).abs() < MERGE_EPS || (b.z2 - acc.z1).abs() < MERGE_EPS;
    let faces_match =
        acc_m.top == b_m.top && acc_m.bottom == b_m.bottom && acc_m.east == b_m.east && acc_m.west == b_m.west;
    if !(same_x_span && adjacent_z && faces_match) {
        return false;
    }
    if b.z1 < acc.z1 {
        acc.z1 = b.z1;
        acc_m.north = b_m.north.clone();
    }
    if b.z2 > acc.z2 {
        acc.z2 = b.z2;
        acc_m.south = b_m.south.clone();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::FLOOR_THICKNESS;

    fn rect(x1: f32, x2: f32, z1: f32, z2: f32) -> Floor {
        Floor {
            x1,
            x2,
            z1,
            z2,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }
    }

    #[test]
    fn floors_merge_across_x_when_only_hidden_caps_differ() {
        let left = (
            rect(0.0, 1.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n", "s", "INNER", "outer_W"),
        );
        let right = (
            rect(1.0, 2.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n", "s", "outer_E", "INNER"),
        );

        let (floors, materials) = merge_floors_with_materials(vec![left, right]);

        assert_eq!(floors.len(), 1);
        assert!((floors[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((floors[0].x2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].west, "outer_W");
        assert_eq!(materials[0].east, "outer_E");
    }

    #[test]
    fn floors_do_not_merge_across_x_when_visible_face_differs() {
        let left = (
            rect(0.0, 1.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
        );
        // North is a visible long face when merging in x.
        let right = (
            rect(1.0, 2.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "DIFFERENT", "s", "e", "w"),
        );

        let (floors, _) = merge_floors_with_materials(vec![left, right]);

        assert_eq!(floors.len(), 2);
    }

    #[test]
    fn floors_merge_across_z_when_only_hidden_caps_differ() {
        let north_rect = (
            rect(0.0, 1.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "outer_N", "INNER", "e", "w"),
        );
        let south_rect = (
            rect(0.0, 1.0, 1.0, 2.0),
            FaceMaterials::from_six("t", "b", "INNER", "outer_S", "e", "w"),
        );

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
        let a = (
            rect(0.0, 1.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n_top", "s_inner", "e_inner", "outer_W"),
        );
        let b = (
            rect(1.0, 2.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n_top", "s_inner", "outer_E", "e_inner"),
        );
        let c = (
            rect(0.0, 1.0, 1.0, 2.0),
            FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "e_inner", "outer_W"),
        );
        let d = (
            rect(1.0, 2.0, 1.0, 2.0),
            FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "outer_E", "e_inner"),
        );

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
