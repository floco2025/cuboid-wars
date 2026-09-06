// Merge same-kind light bridge cells into rectangles, largest first: each
// pass takes the biggest all-free rectangle, so a walkway with spurs keeps
// one collider along its length whichever way it runs. Works in grid space
// (no float epsilon, unlike `merge_floors`/`merge_barriers`) because bridges
// are authored per cell. One collider per rectangle rather than per cell
// matters: the character controller reports a side contact at every
// collider seam, so a per-cell bridge would stutter underfoot.

use std::collections::{BTreeMap, BTreeSet};

use common::protocol::BridgeKindId;

// Half-open cell rectangle `[c0, c1) x [r0, r1)` of a single kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BridgeRect {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    pub(crate) kind: BridgeKindId,
}

impl BridgeRect {
    fn area(&self) -> i32 {
        (self.c1 - self.c0) * (self.r1 - self.r0)
    }
}

#[must_use]
pub(crate) fn merge_light_bridges(cells: &[(i32, i32, BridgeKindId)]) -> Vec<BridgeRect> {
    let mut by_kind: BTreeMap<u16, BTreeSet<(i32, i32)>> = BTreeMap::new();
    for (col, row, kind) in cells {
        by_kind.entry(kind.0).or_default().insert((*row, *col));
    }

    let mut merged = Vec::new();
    for (kind, mut free) in by_kind {
        while let Some(rect) = largest_free_rect(&free, BridgeKindId(kind)) {
            for row in rect.r0..rect.r1 {
                for col in rect.c0..rect.c1 {
                    free.remove(&(row, col));
                }
            }
            merged.push(rect);
        }
    }
    merged
}

// The biggest rectangle of free cells; ties go to the earliest row-major
// anchor, so the result is deterministic.
fn largest_free_rect(free: &BTreeSet<(i32, i32)>, kind: BridgeKindId) -> Option<BridgeRect> {
    let mut best: Option<BridgeRect> = None;
    for &(row, col) in free {
        let candidate = largest_rect_anchored(free, col, row, kind);
        if best.is_none_or(|best| candidate.area() > best.area()) {
            best = Some(candidate);
        }
    }
    best
}

// The biggest free rectangle whose top-left cell is `(col, row)`: every width
// the anchor row allows, paired with the rows that stay free at that width.
fn largest_rect_anchored(free: &BTreeSet<(i32, i32)>, col: i32, row: i32, kind: BridgeKindId) -> BridgeRect {
    let mut best = BridgeRect {
        c0: col,
        r0: row,
        c1: col + 1,
        r1: row + 1,
        kind,
    };
    let mut r1 = i32::MAX;
    for c1 in (col + 1).. {
        if !free.contains(&(row, c1 - 1)) {
            break;
        }
        let mut rows = row + 1;
        while rows < r1 && free.contains(&(rows, c1 - 1)) {
            rows += 1;
        }
        r1 = rows;
        let candidate = BridgeRect {
            c0: col,
            r0: row,
            c1,
            r1,
            kind,
        };
        if candidate.area() > best.area() {
            best = candidate;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY: BridgeKindId = BridgeKindId(0);
    const VOID: BridgeKindId = BridgeKindId(1);

    fn rect(c0: i32, r0: i32, c1: i32, r1: i32, kind: BridgeKindId) -> BridgeRect {
        BridgeRect { c0, r0, c1, r1, kind }
    }

    fn cells(kind: BridgeKindId, cells: &[(i32, i32)]) -> Vec<(i32, i32, BridgeKindId)> {
        cells.iter().map(|&(col, row)| (col, row, kind)).collect()
    }

    #[test]
    fn a_solid_block_becomes_one_rectangle() {
        let cells: Vec<_> = (0..3).flat_map(|row| (0..4).map(move |col| (col, row, SKY))).collect();
        assert_eq!(merge_light_bridges(&cells), [rect(0, 0, 4, 3, SKY)]);
    }

    #[test]
    fn a_row_run_merges_and_a_gap_splits_it() {
        let cells = [(0, 0, SKY), (1, 0, SKY), (3, 0, SKY)];
        assert_eq!(
            merge_light_bridges(&cells),
            [rect(0, 0, 2, 1, SKY), rect(3, 0, 4, 1, SKY)]
        );
    }

    // A two-wide north-south walkway with one spur keeps a single slab along
    // its length; the spur is the only extra collider.
    #[test]
    fn a_walkway_with_a_spur_is_not_split_across_the_walking_direction() {
        let cells = cells(SKY, &[(0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2)]);
        assert_eq!(
            merge_light_bridges(&cells),
            [rect(0, 0, 2, 3, SKY), rect(2, 1, 3, 2, SKY)]
        );
    }

    #[test]
    fn an_east_west_walkway_with_spurs_on_both_sides_stays_one_slab() {
        let mut walkway: Vec<(i32, i32)> = (0..10).flat_map(|col| [(col, 1), (col, 2)]).collect();
        walkway.extend([(2, 0), (7, 3)]);
        let merged = merge_light_bridges(&cells(SKY, &walkway));
        assert_eq!(merged[0], rect(0, 1, 10, 3, SKY));
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn an_l_shape_becomes_two_rectangles() {
        let cells = cells(SKY, &[(0, 0), (1, 0), (2, 0), (0, 1), (0, 2)]);
        let merged = merge_light_bridges(&cells);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged.iter().map(BridgeRect::area).sum::<i32>(), 5);
    }

    #[test]
    fn kinds_never_merge_into_one_rectangle() {
        let cells = [(0, 0, SKY), (1, 0, VOID)];
        assert_eq!(
            merge_light_bridges(&cells),
            [rect(0, 0, 1, 1, SKY), rect(1, 0, 2, 1, VOID)]
        );
    }

    #[test]
    fn duplicate_cells_collapse() {
        let cells = [(2, 2, SKY), (2, 2, SKY)];
        assert_eq!(merge_light_bridges(&cells), [rect(2, 2, 3, 3, SKY)]);
    }
}
