use super::*;

#[test]
fn pong_seeds_after_a_rough_estimate_and_includes_return_latency() {
    let mut sync = TickSync::default();
    assert!(sync.takes_rough_seed());
    assert!(!sync.takes_rough_seed());
    let mut tick = ServerTick(1);
    sync.observe(&mut tick, 100, Duration::from_millis(200), 30);
    assert_eq!(tick.0, 103);
    assert!(!sync.takes_rough_seed());
}

#[test]
fn tick_quantization_is_ignored_but_clock_drift_is_corrected_across_wrap() {
    let mut sync = TickSync::default();
    let mut tick = ServerTick(u32::MAX);
    sync.observe(&mut tick, u32::MAX, Duration::ZERO, 30);
    sync.observe(&mut tick, 0, Duration::ZERO, 30);
    assert_eq!(tick.0, u32::MAX);
    sync.observe(&mut tick, 1, Duration::ZERO, 30);
    assert_eq!(tick.0, 1);
    sync.observe(&mut tick, 0, Duration::ZERO, 30);
    assert_eq!(tick.0, 1);
    sync.observe(&mut tick, u32::MAX, Duration::ZERO, 30);
    assert_eq!(tick.0, u32::MAX);
}
