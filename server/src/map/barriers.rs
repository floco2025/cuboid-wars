// Barrier compilation: authored one-edge barriers become world-space
// `Barrier` records in two passes. `stack_barriers` converts each edge and
// continues a barrier straight up into the same-kind barrier above it when
// no floor slab beside the edge splits them, so a floorless storey gap stays
// closed instead of showing the slot a floor would fill. `merge_barriers`
// then joins collinear neighbours with the same storey span (mirror of
// `walls::merge_walls` with `BarrierKindId` as the grouping key in place of
// `FaceMaterials`).

use std::collections::HashMap;

use super::{mask::Mask, segments::MERGE_EPS};
use common::{
    map::MapGeometry,
    protocol::{Barrier, BarrierKindId},
};

// One authored barrier: its grid edge as `[c0, r0, c1, r1]` and its resolved kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarrierEdge {
    pub edge: [i32; 4],
    pub kind: BarrierKindId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GridEdge {
    Horizontal { row: i32, col: i32 },
    Vertical { row: i32, col: i32 },
}

impl GridEdge {
    fn from_authored([c0, r0, c1, r1]: [i32; 4]) -> Self {
        if r0 == r1 {
            Self::Horizontal {
                row: r0,
                col: c0.min(c1),
            }
        } else {
            Self::Vertical {
                row: r0.min(r1),
                col: c0,
            }
        }
    }

    // The two cells the edge separates, as `(row, col)`; a grid-border edge
    // has one outside the grid.
    fn cells_beside(self) -> [(i32, i32); 2] {
        match self {
            Self::Horizontal { row, col } => [(row - 1, col), (row, col)],
            Self::Vertical { row, col } => [(row, col - 1), (row, col)],
        }
    }
}

fn has_floor_beside(slab_mask: &Mask, edge: GridEdge) -> bool {
    edge.cells_beside().into_iter().any(|(row, col)| {
        usize::try_from(row)
            .ok()
            .zip(usize::try_from(col).ok())
            .and_then(|(row, col)| slab_mask.get(row)?.get(col))
            .is_some_and(|floor| *floor)
    })
}

// Convert every level's authored edges into world-space records, one per
// run of stacked same-kind barriers. `slab_masks[level]` marks the floor
// slabs at that level's y, the ones that would split the storey below from
// it.
#[must_use]
pub(crate) fn stack_barriers(levels: &[Vec<BarrierEdge>], slab_masks: &[Mask], geometry: &MapGeometry) -> Vec<Barrier> {
    assert_eq!(
        levels.len(),
        slab_masks.len(),
        "barrier levels and slab masks differ in count"
    );
    let mut barriers: Vec<Barrier> = Vec::new();
    // Runs that reached the previous level, keyed by edge and kind, as indexes into `barriers`.
    let mut open_runs: HashMap<(GridEdge, BarrierKindId), usize> = HashMap::new();
    for (level_idx, (edges, slab_mask)) in levels.iter().zip(slab_masks).enumerate() {
        let level = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut runs = HashMap::new();
        for barrier in edges {
            let grid_edge = GridEdge::from_authored(barrier.edge);
            let key = (grid_edge, barrier.kind);
            let continued = open_runs
                .get(&key)
                .copied()
                .filter(|_| !has_floor_beside(slab_mask, grid_edge));
            let index = match continued {
                Some(index) => {
                    let run = &mut barriers[index];
                    run.levels += 1;
                    run.height = span_height(geometry, run.levels);
                    index
                }
                None => {
                    barriers.push(barrier_from_edge(barrier, geometry, level));
                    barriers.len() - 1
                }
            };
            runs.insert(key, index);
        }
        open_runs = runs;
    }
    barriers
}

// One-storey world-space segment. Mirrors the wall world-space math
// (cell-corner → world-corner via `MapGeometry`) so a barrier visually
// occupies the same edge as a wall would.
fn barrier_from_edge(barrier: &BarrierEdge, geometry: &MapGeometry, level: u8) -> Barrier {
    let [c0, r0, c1, r1] = barrier.edge;
    Barrier {
        x1: geometry.cell_to_world_x(c0),
        z1: geometry.cell_to_world_z(r0),
        x2: geometry.cell_to_world_x(c1),
        z2: geometry.cell_to_world_z(r1),
        width: geometry.barrier_thickness(),
        y: geometry.level_y(level),
        height: span_height(geometry, 1),
        level,
        levels: 1,
        kind: barrier.kind,
    }
}

// One wall height plus a full storey pitch for every extra level spanned.
fn span_height(geometry: &MapGeometry, levels: u8) -> f32 {
    geometry.wall_height() + f32::from(levels - 1) * geometry.level_height()
}

// Merge collinear adjacent barriers. Two barriers merge when they share:
//   - level, storey span, axis (horizontal/vertical), kind, and perpendicular coordinate;
//   - and the second's near end touches (within epsilon) the first's far end.
//
// Output preserves original entries that don't fit either axis (degenerate /
// zero-length records, which shouldn't exist after validation but are kept
// for parity with `walls::merge_walls`).
#[must_use]
pub(crate) fn merge_barriers(barriers: Vec<Barrier>) -> Vec<Barrier> {
    let mut horizontals: Vec<Barrier> = Vec::new();
    let mut verticals: Vec<Barrier> = Vec::new();
    let mut others: Vec<Barrier> = Vec::new();

    for b in barriers {
        let b = normalize_endpoints(b);
        if (b.z1 - b.z2).abs() < MERGE_EPS {
            horizontals.push(b);
        } else if (b.x1 - b.x2).abs() < MERGE_EPS {
            verticals.push(b);
        } else {
            others.push(b);
        }
    }

    horizontals.sort_by(|a, b| {
        group_key(a)
            .cmp(&group_key(b))
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|a, b| {
        group_key(a)
            .cmp(&group_key(b))
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged: Vec<Barrier> = Vec::new();
    merge_line(horizontals, Axis::Horizontal, &mut merged);
    merge_line(verticals, Axis::Vertical, &mut merged);
    merged.extend(others);
    merged
}

fn group_key(b: &Barrier) -> (u8, u8, u16) {
    (b.level, b.levels, b.kind.0)
}

fn normalize_endpoints(mut b: Barrier) -> Barrier {
    if (b.z1 - b.z2).abs() < MERGE_EPS {
        if b.x1 > b.x2 {
            std::mem::swap(&mut b.x1, &mut b.x2);
        }
    } else if (b.x1 - b.x2).abs() < MERGE_EPS && b.z1 > b.z2 {
        std::mem::swap(&mut b.z1, &mut b.z2);
    }
    b
}

#[derive(Copy, Clone)]
enum Axis {
    Horizontal,
    Vertical,
}

fn merge_line(list: Vec<Barrier>, axis: Axis, out: &mut Vec<Barrier>) {
    let mut iter = list.into_iter();
    let Some(mut cur) = iter.next() else {
        return;
    };
    for b in iter {
        let same_group = group_key(&b) == group_key(&cur);
        let extends = same_group
            && match axis {
                Axis::Horizontal => (cur.z1 - b.z1).abs() < MERGE_EPS && b.x1 <= cur.x2 + MERGE_EPS,
                Axis::Vertical => (cur.x1 - b.x1).abs() < MERGE_EPS && b.z1 <= cur.z2 + MERGE_EPS,
            };
        if extends {
            match axis {
                Axis::Horizontal => {
                    if b.x2 > cur.x2 {
                        cur.x2 = b.x2;
                    }
                }
                Axis::Vertical => {
                    if b.z2 > cur.z2 {
                        cur.z2 = b.z2;
                    }
                }
            }
        } else {
            out.push(cur);
            cur = b;
        }
    }
    out.push(cur);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::{BARRIER_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, geometry};
    use common::protocol::BarrierKindId;

    const RED: BarrierKindId = BarrierKindId(0);
    const BLUE: BarrierKindId = BarrierKindId(1);
    const GREEN: BarrierKindId = BarrierKindId(2);

    fn h(x1: f32, x2: f32, z: f32, kind: BarrierKindId) -> Barrier {
        Barrier {
            x1,
            x2,
            z1: z,
            z2: z,
            level: 0,
            levels: 1,
            kind,
            y: 0.0,
            height: WALL_HEIGHT,
            width: BARRIER_THICKNESS,
        }
    }

    fn v(x: f32, z1: f32, z2: f32, kind: BarrierKindId) -> Barrier {
        Barrier {
            x1: x,
            x2: x,
            z1,
            z2,
            level: 0,
            levels: 1,
            kind,
            y: 0.0,
            height: WALL_HEIGHT,
            width: BARRIER_THICKNESS,
        }
    }

    fn edge(c0: i32, r0: i32, c1: i32, r1: i32, kind: BarrierKindId) -> BarrierEdge {
        BarrierEdge {
            edge: [c0, r0, c1, r1],
            kind,
        }
    }

    fn empty_mask() -> Mask {
        vec![vec![false; 2]; 2]
    }

    fn mask_with_floor(col: i32, row: i32) -> Mask {
        let mut mask = empty_mask();
        mask[row as usize][col as usize] = true;
        mask
    }

    #[test]
    fn stacked_same_kind_barriers_with_no_floor_beside_become_one_record() {
        let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(1, 1, 0, 1, RED)]];
        let masks = vec![empty_mask(), empty_mask()];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 1);
        assert_eq!(barriers[0].level, 0);
        assert_eq!(barriers[0].levels, 2);
        assert_eq!(barriers[0].y, 0.0);
        assert_eq!(barriers[0].height, LEVEL_HEIGHT + WALL_HEIGHT);
    }

    #[test]
    fn a_floor_slab_beside_the_edge_keeps_the_storeys_apart() {
        let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(0, 1, 1, 1, RED)]];
        let masks = vec![empty_mask(), mask_with_floor(0, 1)];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 2);
        assert!(barriers.iter().all(|b| b.levels == 1 && b.height == WALL_HEIGHT));
        assert_eq!(barriers[1].level, 1);
        assert_eq!(barriers[1].y, LEVEL_HEIGHT);
    }

    #[test]
    fn a_floor_on_either_side_of_the_edge_splits_the_stack() {
        let levels = vec![vec![edge(1, 0, 1, 1, RED)], vec![edge(1, 0, 1, 1, RED)]];
        let masks = vec![empty_mask(), mask_with_floor(0, 0)];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 2);
    }

    #[test]
    fn stacks_do_not_cross_kinds() {
        let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(0, 1, 1, 1, BLUE)]];
        let masks = vec![empty_mask(), empty_mask()];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 2);
    }

    #[test]
    fn a_run_spans_every_floorless_storey_and_restarts_past_a_floor() {
        let levels = vec![
            vec![edge(0, 1, 1, 1, RED)],
            vec![edge(0, 1, 1, 1, RED)],
            vec![edge(0, 1, 1, 1, RED)],
            vec![edge(0, 1, 1, 1, RED)],
        ];
        let masks = vec![empty_mask(), empty_mask(), empty_mask(), mask_with_floor(0, 0)];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 2);
        assert_eq!((barriers[0].level, barriers[0].levels), (0, 3));
        assert_eq!(barriers[0].height, 2.0 * LEVEL_HEIGHT + WALL_HEIGHT);
        assert_eq!((barriers[1].level, barriers[1].levels), (3, 1));
    }

    #[test]
    fn a_missing_storey_ends_the_run() {
        let levels = vec![vec![edge(0, 1, 1, 1, RED)], Vec::new(), vec![edge(0, 1, 1, 1, RED)]];
        let masks = vec![empty_mask(), empty_mask(), empty_mask()];

        let barriers = stack_barriers(&levels, &masks, &geometry(2, 2));

        assert_eq!(barriers.len(), 2);
        assert!(barriers.iter().all(|b| b.levels == 1));
    }

    #[test]
    fn merges_adjacent_same_kind_horizontals() {
        let merged = merge_barriers(vec![
            h(0.0, 1.0, 0.0, RED),
            h(1.0, 2.0, 0.0, RED),
            h(2.0, 3.0, 0.0, RED),
        ]);
        assert_eq!(merged.len(), 1);
        assert!((merged[0].x1 - 0.0).abs() < MERGE_EPS);
        assert!((merged[0].x2 - 3.0).abs() < MERGE_EPS);
    }

    #[test]
    fn does_not_merge_across_kind_change() {
        let merged = merge_barriers(vec![
            h(0.0, 1.0, 0.0, RED),
            h(1.0, 2.0, 0.0, RED),
            h(2.0, 3.0, 0.0, BLUE),
            h(3.0, 4.0, 0.0, RED),
        ]);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn does_not_merge_across_row_change() {
        let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), h(0.0, 1.0, 1.0, RED)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn does_not_merge_with_gap() {
        let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), h(2.0, 3.0, 0.0, RED)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merges_verticals() {
        let merged = merge_barriers(vec![v(0.0, 0.0, 1.0, GREEN), v(0.0, 1.0, 2.0, GREEN)]);
        assert_eq!(merged.len(), 1);
        assert!((merged[0].z1 - 0.0).abs() < MERGE_EPS);
        assert!((merged[0].z2 - 2.0).abs() < MERGE_EPS);
    }

    #[test]
    fn does_not_merge_across_axis() {
        let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), v(0.0, 0.0, 1.0, RED)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn does_not_merge_across_level() {
        let mut b0 = h(0.0, 1.0, 0.0, RED);
        let mut b1 = h(1.0, 2.0, 0.0, RED);
        b0.level = 0;
        b1.level = 1;
        let merged = merge_barriers(vec![b0, b1]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn does_not_merge_across_storey_span() {
        let mut tall = h(0.0, 1.0, 0.0, RED);
        tall.levels = 2;
        tall.height = LEVEL_HEIGHT + WALL_HEIGHT;
        let merged = merge_barriers(vec![tall, h(1.0, 2.0, 0.0, RED)]);
        assert_eq!(merged.len(), 2);
    }
}
