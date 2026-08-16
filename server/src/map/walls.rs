// Wall generation: turn canonical grid-line wall flags into world-space `Wall`
// segments, with corner-correct insets and extensions for T-junctions and
// L-corners. The map compiler sets the cell flags from explicit wall
// edges; this module emits the world-space segments.

use super::{
    edges::{has_horizontal_edge, has_vertical_edge},
    material_rules::MaterialRules,
    segments::{MERGE_EPS, horizontal_wall_segment, vertical_wall_segment},
};
use crate::map::EdgeGrid;
use common::{constants::*, map::MapGeometry, protocol::FaceMaterials, protocol::Wall};

// Generate individual wall segments (no merging) with gap-filling extensions,
// tagging each wall with `level`.
#[must_use]
pub fn generate_walls(edge_grid: &EdgeGrid, geometry: &MapGeometry, level: u8) -> Vec<Wall> {
    let grid_cols = geometry.grid_cols;
    let grid_rows = geometry.grid_rows;
    let mut walls = Vec::new();

    // Process horizontal walls (north/south edges)
    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !has_horizontal_edge(edge_grid, row, col) {
                continue;
            }

            let segment = horizontal_wall_segment(edge_grid, row, col, geometry);
            walls.push(Wall {
                x1: segment.x1,
                z1: segment.z,
                x2: segment.x2,
                z2: segment.z,
                width: WALL_THICKNESS,
                level,
            });
        }
    }

    // Process vertical walls (west/east edges)
    for col in 0..=grid_cols {
        for row in 0..grid_rows {
            if !has_vertical_edge(edge_grid, row, col) {
                continue;
            }

            let segment = vertical_wall_segment(edge_grid, row, col, geometry);
            walls.push(Wall {
                x1: segment.x,
                z1: segment.z1,
                x2: segment.x,
                z2: segment.z2,
                width: WALL_THICKNESS,
                level,
            });
        }
    }

    walls
}

// ============================================================================
// Wall Merging
// ============================================================================

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

// Merge collinear walls that are adjacent or overlapping. Compares only the
// faces that remain visible after merging: for a horizontal wall those are
// top/bottom/north/south (east/west are end caps; the abutting pair becomes
// interior); for a vertical wall those are top/bottom/east/west.
//
// The merged wall keeps the long-side / top-bottom materials (matched by
// definition) and takes its outer end caps from the source walls at each
// extremity.
fn merge_walls_line(list: Vec<(Wall, FaceMaterials)>, is_horizontal: bool, out: &mut Vec<(Wall, FaceMaterials)>) {
    let mut iter = list.into_iter();
    if let Some((mut cur_wall, mut cur_materials)) = iter.next() {
        for (w, materials) in iter {
            if is_horizontal {
                if (cur_wall.z1 - w.z1).abs() < MERGE_EPS
                    && (cur_wall.width - w.width).abs() < MERGE_EPS
                    && horizontal_visible_faces_match(&cur_materials, &materials)
                    && w.x1 <= cur_wall.x2 + MERGE_EPS
                {
                    if w.x2 > cur_wall.x2 {
                        // The new wall extends the merged span eastward — its
                        // east face becomes the merged east cap.
                        cur_wall.x2 = w.x2;
                        cur_materials.east = materials.east;
                    }
                    continue;
                }
            } else if (cur_wall.x1 - w.x1).abs() < MERGE_EPS
                && (cur_wall.width - w.width).abs() < MERGE_EPS
                && vertical_visible_faces_match(&cur_materials, &materials)
                && w.z1 <= cur_wall.z2 + MERGE_EPS
            {
                if w.z2 > cur_wall.z2 {
                    // The new wall extends the merged span south (larger z) —
                    // its south face becomes the merged south cap.
                    cur_wall.z2 = w.z2;
                    cur_materials.south = materials.south;
                }
                continue;
            }
            out.push((cur_wall, cur_materials));
            cur_wall = w;
            cur_materials = materials;
        }
        out.push((cur_wall, cur_materials));
    }
}

fn horizontal_visible_faces_match(a: &FaceMaterials, b: &FaceMaterials) -> bool {
    a.top == b.top && a.bottom == b.bottom && a.north == b.north && a.south == b.south
}

fn vertical_visible_faces_match(a: &FaceMaterials, b: &FaceMaterials) -> bool {
    a.top == b.top && a.bottom == b.bottom && a.east == b.east && a.west == b.west
}

// Merge adjacent collinear walls into longer segments, returning the merged
// walls alongside their visible-face composite materials.
pub fn merge_walls(walls: Vec<Wall>, assets: &MaterialRules) -> (Vec<Wall>, Vec<FaceMaterials>) {
    let paired: Vec<(Wall, FaceMaterials)> = walls
        .into_iter()
        .map(|w| {
            let m = assets.materials_for_wall(&w);
            (w, m)
        })
        .collect();
    merge_walls_with_materials(paired)
}

// Inner merge driver — operates on `(Wall, FaceMaterials)` pairs so tests can
// supply face materials directly without needing a full `MaterialRules`.
fn merge_walls_with_materials(walls: Vec<(Wall, FaceMaterials)>) -> (Vec<Wall>, Vec<FaceMaterials>) {
    let mut horizontals = Vec::new();
    let mut verticals = Vec::new();
    let mut others = Vec::new();

    for (w, m) in walls {
        let w = normalize_wall(w);
        if (w.z1 - w.z2).abs() < MERGE_EPS {
            horizontals.push((w, m));
        } else if (w.x1 - w.x2).abs() < MERGE_EPS {
            verticals.push((w, m));
        } else {
            others.push((w, m));
        }
    }

    horizontals.sort_by(|(a, _), (b, _)| {
        a.z1.partial_cmp(&b.z1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|(a, _), (b, _)| {
        a.x1.partial_cmp(&b.x1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged: Vec<(Wall, FaceMaterials)> = Vec::new();
    merge_walls_line(horizontals, true, &mut merged);
    merge_walls_line(verticals, false, &mut merged);
    merged.extend(others);

    let mut walls_out = Vec::with_capacity(merged.len());
    let mut materials_out = Vec::with_capacity(merged.len());
    for (w, m) in merged {
        walls_out.push(w);
        materials_out.push(m);
    }
    (walls_out, materials_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h_wall(x1: f32, x2: f32, z: f32) -> Wall {
        Wall {
            x1,
            x2,
            z1: z,
            z2: z,
            width: WALL_THICKNESS,
            level: 0,
        }
    }

    fn v_wall(x: f32, z1: f32, z2: f32) -> Wall {
        Wall {
            x1: x,
            x2: x,
            z1,
            z2,
            width: WALL_THICKNESS,
            level: 0,
        }
    }

    #[test]
    fn horizontal_walls_merge_when_only_hidden_caps_differ() {
        // Two adjacent horizontal wall edges. Long faces and top/bottom match;
        // their abutting east/west caps differ — those become interior on
        // merge and shouldn't block it.
        let left = (
            h_wall(0.0, 1.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "INNER_E", "outer_W"),
        );
        let right = (
            h_wall(1.0, 2.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "outer_E", "INNER_W"),
        );

        let (walls, materials) = merge_walls_with_materials(vec![left, right]);

        assert_eq!(walls.len(), 1);
        assert!((walls[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((walls[0].x2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].west, "outer_W");
        assert_eq!(materials[0].east, "outer_E");
    }

    #[test]
    fn horizontal_walls_do_not_merge_when_visible_face_differs() {
        let left = (
            h_wall(0.0, 1.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
        );
        // North face differs — it's a long visible face for a horizontal wall.
        let right = (
            h_wall(1.0, 2.0, 0.0),
            FaceMaterials::from_six("t", "b", "DIFFERENT", "s", "e", "w"),
        );

        let (walls, _) = merge_walls_with_materials(vec![left, right]);

        assert_eq!(walls.len(), 2);
    }

    #[test]
    fn vertical_walls_merge_when_only_hidden_caps_differ() {
        let north = (
            v_wall(0.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "outer_N", "INNER_S", "e", "w"),
        );
        let south = (
            v_wall(0.0, 1.0, 2.0),
            FaceMaterials::from_six("t", "b", "INNER_N", "outer_S", "e", "w"),
        );

        let (walls, materials) = merge_walls_with_materials(vec![north, south]);

        assert_eq!(walls.len(), 1);
        assert!((walls[0].z1 - 0.0).abs() < MERGE_EPS);
        assert!((walls[0].z2 - 2.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].north, "outer_N");
        assert_eq!(materials[0].south, "outer_S");
    }

    #[test]
    fn vertical_walls_do_not_merge_when_visible_face_differs() {
        let north = (
            v_wall(0.0, 0.0, 1.0),
            FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
        );
        // East face differs — it's a long visible face for a vertical wall.
        let south = (
            v_wall(0.0, 1.0, 2.0),
            FaceMaterials::from_six("t", "b", "n", "s", "DIFFERENT", "w"),
        );

        let (walls, _) = merge_walls_with_materials(vec![north, south]);

        assert_eq!(walls.len(), 2);
    }

    #[test]
    fn three_horizontal_walls_chain_with_outer_caps() {
        let a = (
            h_wall(0.0, 1.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "ae", "leftmost_W"),
        );
        let b = (
            h_wall(1.0, 2.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "be", "bw"),
        );
        let c = (
            h_wall(2.0, 3.0, 0.0),
            FaceMaterials::from_six("t", "b", "n", "s", "rightmost_E", "cw"),
        );

        let (walls, materials) = merge_walls_with_materials(vec![a, b, c]);

        assert_eq!(walls.len(), 1);
        assert!((walls[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((walls[0].x2 - 3.0).abs() < MERGE_EPS);
        assert_eq!(materials[0].west, "leftmost_W");
        assert_eq!(materials[0].east, "rightmost_E");
    }
}
