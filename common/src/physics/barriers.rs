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
#[path = "tests/barriers_passable_barrier_kinds.rs"]
mod passable_barrier_kinds_tests;
