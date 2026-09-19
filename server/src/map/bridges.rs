// Merge the light bridge cells of each field into rectangles, largest first: each
// pass takes the biggest all-free rectangle, so a walkway with spurs keeps
// one collider along its length whichever way it runs. Works in grid space
// (no float epsilon, unlike `merge_floors`/`merge_barriers`) because bridges
// are authored per cell. One collider per rectangle rather than per cell
// matters: the character controller reports a side contact at every
// collider seam, so a per-cell bridge would stutter underfoot.

use std::collections::{BTreeMap, BTreeSet};

use common::protocol::FieldId;

// Half-open cell rectangle `[c0, c1) x [r0, r1)` of a single field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BridgeRect {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    pub(crate) field: FieldId,
}

impl BridgeRect {
    fn area(&self) -> i32 {
        (self.c1 - self.c0) * (self.r1 - self.r0)
    }
}

#[must_use]
pub(crate) fn merge_light_bridges(cells: &[(i32, i32, FieldId)]) -> Vec<BridgeRect> {
    let mut by_field: BTreeMap<FieldId, BTreeSet<(i32, i32)>> = BTreeMap::new();
    for (col, row, field) in cells {
        by_field.entry(*field).or_default().insert((*row, *col));
    }

    let mut merged = Vec::new();
    for (field, mut free) in by_field {
        while let Some(rect) = largest_free_rect(&free, field) {
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
fn largest_free_rect(free: &BTreeSet<(i32, i32)>, field: FieldId) -> Option<BridgeRect> {
    let mut best: Option<BridgeRect> = None;
    for &(row, col) in free {
        let candidate = largest_rect_anchored(free, col, row, field);
        if best.is_none_or(|best| candidate.area() > best.area()) {
            best = Some(candidate);
        }
    }
    best
}

// The biggest free rectangle whose top-left cell is `(col, row)`: every width
// the anchor row allows, paired with the rows that stay free at that width.
fn largest_rect_anchored(free: &BTreeSet<(i32, i32)>, col: i32, row: i32, field: FieldId) -> BridgeRect {
    let mut best = BridgeRect {
        c0: col,
        r0: row,
        c1: col + 1,
        r1: row + 1,
        field,
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
            field,
        };
        if candidate.area() > best.area() {
            best = candidate;
        }
    }
    best
}

#[cfg(test)]
#[path = "tests/bridges.rs"]
mod tests;
