use super::*;
use common::protocol::PlayerId;

fn burst(target: u32) -> Option<ActorBeam> {
    Some(ActorBeam {
        target: PlayerId(target),
        started_tick: 10,
        remaining_secs: 2.0,
    })
}

#[test]
fn beam_recovers_from_snapshots_without_accepting_stale_cues() {
    let mut beam = ActorBeamState::default();
    beam.apply(10, burst(1));
    beam.apply(12, None);
    beam.apply(11, burst(2));
    assert_eq!(beam.active(12, &NetworkConfig::default()), None);
    beam.apply(13, burst(2));
    beam.apply(12, None);
    assert_eq!(beam.active(13, &NetworkConfig::default()), burst(2));
    beam.apply(14, None);
    assert_eq!(beam.active(14, &NetworkConfig::default()), None);
}

#[test]
fn beam_orders_and_expires_across_tick_wraparound() {
    let mut beam = ActorBeamState::default();
    beam.apply(u32::MAX, burst(1));
    beam.apply(0, burst(2));
    beam.apply(u32::MAX, None);
    assert_eq!(beam.active(0, &NetworkConfig::default()), burst(2));
    let active = beam.active(30, &NetworkConfig::default()).expect("beam expired early");
    assert!((active.remaining_secs - 1.0).abs() < 0.0001);
    assert_eq!(beam.active(61, &NetworkConfig::default()), None);
}

#[test]
fn late_snapshot_uses_remaining_time_without_extending_the_burst() {
    let mut beam = ActorBeamState::default();
    beam.apply(
        40,
        Some(ActorBeam {
            target: PlayerId(1),
            started_tick: 10,
            remaining_secs: 1.0,
        }),
    );
    assert!(
        (beam
            .active(55, &NetworkConfig::default())
            .expect("beam expired early")
            .remaining_secs
            - 0.5)
            .abs()
            < 0.0001
    );
    assert_eq!(beam.active(71, &NetworkConfig::default()), None);
    beam.apply(
        60,
        Some(ActorBeam {
            target: PlayerId(1),
            started_tick: 10,
            remaining_secs: 10.0 * NetworkConfig::default().tick_secs(),
        }),
    );
    assert_eq!(beam.active(71, &NetworkConfig::default()), None);
}
