// Wall generation: turn canonical grid-line wall flags into world-space `Wall`
// segments, with corner-correct insets and extensions for T-junctions and
// L-corners. The map compiler sets the cell flags from explicit wall
// edges; this module emits the world-space segments.

use super::{
    edges::{has_horizontal_edge, has_vertical_edge},
    segments::{horizontal_wall_segment, vertical_wall_segment},
};
use crate::resources::EdgeGrid;
use common::{assets::AssetRules, constants::*, protocol::Wall};

// Epsilon for merging adjacent walls.
const MERGE_EPS: f32 = 0.01;

// Generate individual wall segments (no merging) with gap-filling extensions,
// tagging each wall with `level`.
#[must_use]
pub fn generate_walls(edge_grid: &EdgeGrid, grid_cols: i32, grid_rows: i32, level: u8) -> Vec<Wall> {
    let mut walls = Vec::new();

    // Process horizontal walls (north/south edges)
    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !has_horizontal_edge(edge_grid, row, col) {
                continue;
            }

            let segment = horizontal_wall_segment(edge_grid, row, col, grid_cols, grid_rows);
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

            let segment = vertical_wall_segment(edge_grid, row, col, grid_cols, grid_rows);
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

// Merge collinear walls that are adjacent or overlapping.
fn merge_walls_line(list: Vec<Wall>, is_horizontal: bool, assets: &AssetRules, out: &mut Vec<Wall>) {
    let mut iter = list.into_iter();
    if let Some(mut cur) = iter.next() {
        let mut cur_material = assets.material_for_wall(&cur);
        for w in iter {
            let material = assets.material_for_wall(&w);
            if is_horizontal {
                if (cur.z1 - w.z1).abs() < MERGE_EPS
                    && (cur.width - w.width).abs() < MERGE_EPS
                    && cur_material == material
                    && w.x1 <= cur.x2 + MERGE_EPS
                {
                    cur.x2 = cur.x2.max(w.x2);
                    continue;
                }
            } else if (cur.x1 - w.x1).abs() < MERGE_EPS
                && (cur.width - w.width).abs() < MERGE_EPS
                && cur_material == material
                && w.z1 <= cur.z2 + MERGE_EPS
            {
                cur.z2 = cur.z2.max(w.z2);
                continue;
            }
            out.push(cur);
            cur = w;
            cur_material = material;
        }
        out.push(cur);
    }
}

// Merge adjacent collinear walls into longer segments
pub fn merge_walls(walls: Vec<Wall>, assets: &AssetRules) -> Vec<Wall> {
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
    merge_walls_line(horizontals, true, assets, &mut merged);
    merge_walls_line(verticals, false, assets, &mut merged);
    merged.extend(others);
    merged
}
