// Merge same-kind light bridge cells into maximal rectangles: horizontal runs
// first, then vertically stacked runs sharing a column span. Works in grid
// space (no float epsilon, unlike `merge_floors`/`merge_barriers`) because
// bridges are authored per cell. One collider per rectangle rather than per
// cell matters: the character controller reports a side contact at every
// collider seam, so a per-cell bridge would stutter underfoot.

use std::collections::BTreeMap;

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

#[must_use]
pub(crate) fn merge_light_bridges(cells: &[(i32, i32, BridgeKindId)]) -> Vec<BridgeRect> {
    let mut by_kind: BTreeMap<u16, Vec<(i32, i32)>> = BTreeMap::new();
    for (col, row, kind) in cells {
        by_kind.entry(kind.0).or_default().push((*col, *row));
    }

    let mut merged = Vec::new();
    for (kind, mut cells) in by_kind {
        cells.sort_unstable_by_key(|&(col, row)| (row, col));
        cells.dedup();
        merged.extend(merge_one_kind(&cells, BridgeKindId(kind)));
    }
    merged
}

// `cells` must be sorted row-major and deduplicated.
fn merge_one_kind(cells: &[(i32, i32)], kind: BridgeKindId) -> Vec<BridgeRect> {
    let mut runs: Vec<BridgeRect> = Vec::new();
    for &(col, row) in cells {
        match runs.last_mut() {
            Some(run) if run.r0 == row && run.c1 == col => run.c1 = col + 1,
            _ => runs.push(BridgeRect {
                c0: col,
                r0: row,
                c1: col + 1,
                r1: row + 1,
                kind,
            }),
        }
    }

    let mut stacked: Vec<BridgeRect> = Vec::new();
    for run in runs {
        match stacked
            .iter_mut()
            .find(|prev| prev.c0 == run.c0 && prev.c1 == run.c1 && prev.r1 == run.r0)
        {
            Some(prev) => prev.r1 = run.r1,
            None => stacked.push(run),
        }
    }
    stacked
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY: BridgeKindId = BridgeKindId(0);
    const VOID: BridgeKindId = BridgeKindId(1);

    fn rect(c0: i32, r0: i32, c1: i32, r1: i32, kind: BridgeKindId) -> BridgeRect {
        BridgeRect { c0, r0, c1, r1, kind }
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

    #[test]
    fn rows_stack_only_when_their_spans_match() {
        let cells = [(0, 0, SKY), (1, 0, SKY), (0, 1, SKY)];
        assert_eq!(
            merge_light_bridges(&cells),
            [rect(0, 0, 2, 1, SKY), rect(0, 1, 1, 2, SKY)]
        );
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
