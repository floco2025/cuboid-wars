// Merge collinear adjacent barriers (mirror of `walls::merge_walls` with
// `BarrierKindId` as the grouping key in place of `FaceMaterials`). The
// `BarrierDef` → world-space `Barrier` conversion lives in `definition::compile`
// — by the time we get here every barrier is already a world-space segment.

use super::segments::MERGE_EPS;
use common::protocol::Barrier;

// Merge collinear adjacent barriers. Two barriers merge when they share:
//   - level, axis (horizontal/vertical), kind, and perpendicular coordinate;
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
        (a.level, a.kind.0)
            .cmp(&(b.level, b.kind.0))
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|a, b| {
        (a.level, a.kind.0)
            .cmp(&(b.level, b.kind.0))
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged: Vec<Barrier> = Vec::new();
    merge_line(horizontals, Axis::Horizontal, &mut merged);
    merge_line(verticals, Axis::Vertical, &mut merged);
    merged.extend(others);
    merged
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
        let same_group = b.level == cur.level && b.kind == cur.kind;
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
    use crate::test_geometry::{BARRIER_THICKNESS, WALL_HEIGHT};
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
            kind,
            y: 0.0,
            height: WALL_HEIGHT,
            width: BARRIER_THICKNESS,
        }
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
}
