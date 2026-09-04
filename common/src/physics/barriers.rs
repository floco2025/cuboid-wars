use crate::protocol::BarrierKindId;

// Merge per-player `held_keys` with the globally `open_kinds` (currently held
// open by pressure plates, `PlateState.open_barrier_kinds`) into the slice
// that `step_character_movement` treats as "barriers I can pass through".
// One source of truth used by both server-authoritative movement and
// client-side prediction — keeps the two sides in agreement about what's
// passable.
#[must_use]
pub fn passable_barrier_kinds(held_keys: &[BarrierKindId], open_kinds: &[BarrierKindId]) -> Vec<BarrierKindId> {
    if open_kinds.is_empty() {
        return held_keys.to_vec();
    }
    let mut combined: Vec<BarrierKindId> = held_keys.to_vec();
    for k in open_kinds {
        if !combined.contains(k) {
            combined.push(*k);
        }
    }
    combined
}

#[cfg(test)]
mod passable_barrier_kinds_tests {
    use super::*;

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(passable_barrier_kinds(&[], &[]).is_empty());
    }

    #[test]
    fn empty_open_returns_held_keys_clone() {
        let held = [BarrierKindId(0), BarrierKindId(2)];
        assert_eq!(passable_barrier_kinds(&held, &[]), held);
    }

    #[test]
    fn empty_held_returns_open_kinds_clone() {
        let open = [BarrierKindId(1), BarrierKindId(3)];
        assert_eq!(passable_barrier_kinds(&[], &open), open);
    }

    #[test]
    fn merge_dedupes_overlap() {
        let held = [BarrierKindId(0), BarrierKindId(1)];
        let open = [BarrierKindId(1), BarrierKindId(2)];
        let merged = passable_barrier_kinds(&held, &open);
        assert_eq!(merged.len(), 3, "{merged:?} should have 3 unique kinds");
        for k in [BarrierKindId(0), BarrierKindId(1), BarrierKindId(2)] {
            assert!(merged.contains(&k), "missing {k:?} in {merged:?}");
        }
    }

    #[test]
    fn held_kinds_precede_added_open_kinds() {
        // Order isn't a contract for the collision filter (which iterates),
        // but documenting current behavior makes regressions visible.
        let held = [BarrierKindId(5)];
        let open = [BarrierKindId(2)];
        assert_eq!(
            passable_barrier_kinds(&held, &open),
            [BarrierKindId(5), BarrierKindId(2)]
        );
    }
}
