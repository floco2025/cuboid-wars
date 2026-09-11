// Barrier compilation: authored one-edge barriers become carrier-local
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
    protocol::{Barrier, BarrierKindId, CarrierId, SwitchId},
};

// One authored barrier: its grid edge as `[c0, r0, c1, r1]` and its resolved kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarrierEdge {
    pub switch: Option<SwitchId>,
    pub switch_inverted: bool,

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

// Convert every level's authored edges into carrier-local records, one per
// run of stacked same-kind barriers. `slab_masks[level]` marks the floor
// slabs at that level's y, the ones that would split the storey below from
// it.
#[must_use]
pub(crate) fn stack_barriers(
    levels: &[Vec<BarrierEdge>],
    slab_masks: &[Mask],
    geometry: &MapGeometry,
    carrier: CarrierId,
) -> Vec<Barrier> {
    assert_eq!(
        levels.len(),
        slab_masks.len(),
        "barrier levels and slab masks differ in count"
    );
    let mut barriers: Vec<Barrier> = Vec::new();
    // Runs that reached the previous level, keyed by edge and kind, as indexes into `barriers`.
    let mut open_runs: HashMap<(GridEdge, BarrierKindId, Option<SwitchId>, bool), usize> = HashMap::new();
    for (level_idx, (edges, slab_mask)) in levels.iter().zip(slab_masks).enumerate() {
        let level = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut runs = HashMap::new();
        for barrier in edges {
            let grid_edge = GridEdge::from_authored(barrier.edge);
            let key = (grid_edge, barrier.kind, barrier.switch, barrier.switch_inverted);
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
                    barriers.push(barrier_from_edge(barrier, geometry, level, carrier));
                    barriers.len() - 1
                }
            };
            runs.insert(key, index);
        }
        open_runs = runs;
    }
    barriers
}

// Use the wall's grid-to-carrier conversion so barriers occupy the same edges.
fn barrier_from_edge(barrier: &BarrierEdge, geometry: &MapGeometry, level: u8, carrier: CarrierId) -> Barrier {
    let [c0, r0, c1, r1] = barrier.edge;
    Barrier {
        id: Default::default(),
        switch: barrier.switch,
        switch_inverted: barrier.switch_inverted,

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
        carrier,
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

fn group_key(b: &Barrier) -> (u8, u8, u16, Option<SwitchId>, bool) {
    (b.level, b.levels, b.kind.0, b.switch, b.switch_inverted)
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
#[path = "tests/barriers.rs"]
mod tests;
