use super::*;

#[test]
fn sorted_lookups_find_switches_and_runs() {
    let running = CarrierRun {
        running: true,
        run_ticks: 5,
        since_tick: 10,
    };
    let mut state = PlateState {
        active_switches: vec![SwitchId(3), SwitchId(1)],
        open_barrier_kinds: vec![BarrierKindId(2), BarrierKindId(0)],
        powered_bridge_kinds: vec![BridgeKindId(1)],
        carrier_runs: vec![(CarrierId(2), running), (CarrierId(1), CarrierRun::STOPPED)],
    };
    state.sort();
    assert_eq!(state.active_switches, [SwitchId(1), SwitchId(3)]);
    assert_eq!(state.open_barrier_kinds, [BarrierKindId(0), BarrierKindId(2)]);
    assert!(state.is_active(SwitchId(3)));
    assert!(!state.is_active(SwitchId(2)));
    assert_eq!(state.carrier_run(CarrierId(2)), Some(running));
    assert_eq!(state.carrier_run(CarrierId(1)), Some(CarrierRun::STOPPED));
    assert_eq!(state.carrier_run(CarrierId(3)), None);
    let bytes = bincode::encode_to_vec(&state, bincode::config::standard()).expect("plate state encoding failed");
    let (decoded, _): (PlateState, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("plate state decoding failed");
    assert_eq!(decoded, state);
}
