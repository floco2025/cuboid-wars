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
