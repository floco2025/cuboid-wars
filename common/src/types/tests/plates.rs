use super::*;

#[test]
fn held_purposes_round_trip_sorted() {
    let state = PlateState::from_held([
        HeldPurpose::Bridge(BridgeKindId(1)),
        HeldPurpose::Barrier(BarrierKindId(2)),
        HeldPurpose::Barrier(BarrierKindId(0)),
    ]);
    assert_eq!(state.open_barrier_kinds, [BarrierKindId(0), BarrierKindId(2)]);
    assert_eq!(state.powered_bridge_kinds, [BridgeKindId(1)]);
    assert_eq!(
        state.held().collect::<Vec<_>>(),
        [
            HeldPurpose::Barrier(BarrierKindId(0)),
            HeldPurpose::Barrier(BarrierKindId(2)),
            HeldPurpose::Bridge(BridgeKindId(1)),
        ]
    );
    assert!(state.contains(HeldPurpose::Bridge(BridgeKindId(1))));
    assert!(!state.contains(HeldPurpose::Barrier(BarrierKindId(1))));
    assert_eq!(PlateState::from_held(state.held()), state);
}
