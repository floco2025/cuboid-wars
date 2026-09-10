use super::*;

fn pong(tick: u32, sent_at: Duration) -> SPong {
    SPong {
        tick,
        timestamp_nanos: sent_at.as_nanos() as u64,
    }
}

#[test]
fn pong_measures_against_its_echoed_send_time_including_a_ping_sent_at_startup() {
    let mut time = Time::default();
    time.advance_by(Duration::from_millis(10));
    let mut rtt = RoundTripTime::default();
    let mut sync = TickSync::default();
    let mut tick = ServerTick(0);
    apply_pong(
        &time,
        &mut rtt,
        &mut sync,
        &mut tick,
        pong(100, Duration::ZERO),
        &NetworkConfig::default(),
    );
    assert_eq!(tick.0, 100);
    assert_eq!(rtt.rtt, Duration::from_millis(10));
    tick.0 = 101;
    apply_pong(
        &time,
        &mut rtt,
        &mut sync,
        &mut tick,
        pong(100, Duration::ZERO),
        &NetworkConfig::default(),
    );
    assert_eq!(tick.0, 101);
}

#[test]
fn a_round_trip_longer_than_the_ping_interval_still_updates_rtt_and_the_clock() {
    let mut time = Time::default();
    time.advance_by(Duration::from_millis(1200));
    let mut rtt = RoundTripTime::default();
    let mut sync = TickSync::default();
    let mut tick = ServerTick(0);
    // A second ping went out at 1.0 s; the first pong arrives after it.
    apply_pong(
        &time,
        &mut rtt,
        &mut sync,
        &mut tick,
        pong(100, Duration::ZERO),
        &NetworkConfig::default(),
    );
    assert_eq!(rtt.rtt, Duration::from_millis(1200));
    assert_eq!(tick.0, 118);
}

#[test]
fn ancient_or_future_echoes_are_ignored() {
    let mut time = Time::default();
    time.advance_by(Duration::from_secs(10));
    let mut rtt = RoundTripTime::default();
    let mut sync = TickSync::default();
    let mut tick = ServerTick(0);
    for sent_at in [Duration::ZERO, Duration::from_secs(11)] {
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong(100, sent_at),
            &NetworkConfig::default(),
        );
    }
    assert_eq!(tick.0, 0);
    assert_eq!(rtt.rtt, Duration::ZERO);
    assert!(rtt.measurements.is_empty());
}

#[test]
fn delayed_pong_updates_rtt_without_shifting_the_clock() {
    let mut time = Time::default();
    time.advance_by(Duration::from_millis(200));
    let mut rtt = RoundTripTime {
        measurements: [Duration::from_millis(10)].into(),
        ..default()
    };
    let mut sync = TickSync::default();
    let mut tick = ServerTick(101);
    apply_pong(
        &time,
        &mut rtt,
        &mut sync,
        &mut tick,
        pong(100, Duration::ZERO),
        &NetworkConfig::default(),
    );
    assert_eq!(tick.0, 101);
    assert_eq!(rtt.rtt, Duration::from_millis(105));
}
