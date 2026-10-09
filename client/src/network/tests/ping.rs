use super::*;

// The round trip and server tick a client holds, taking pongs at one moment.
struct Clock {
    time: Time,
    rtt: RoundTripTime,
    sync: TickSync,
    tick: ServerTick,
}

impl Clock {
    fn at(now: Duration, tick: u32) -> Self {
        let mut time = Time::default();
        time.advance_by(now);
        Self {
            time,
            rtt: RoundTripTime::default(),
            sync: TickSync::default(),
            tick: ServerTick(tick),
        }
    }

    fn pong(&mut self, tick: u32, sent_at: Duration) {
        apply_pong(
            &self.time,
            &mut self.rtt,
            &mut self.sync,
            &mut self.tick,
            SPong {
                tick,
                timestamp_nanos: sent_at.as_nanos() as u64,
            },
            &NetworkConfig::default(),
        );
    }
}

#[test]
fn pong_measures_against_its_echoed_send_time_including_a_ping_sent_at_startup() {
    let mut clock = Clock::at(Duration::from_millis(10), 0);
    clock.pong(100, Duration::ZERO);
    assert_eq!(clock.tick.0, 100);
    assert_eq!(clock.rtt.rtt, Duration::from_millis(10));
    clock.tick.0 = 101;
    clock.pong(100, Duration::ZERO);
    assert_eq!(clock.tick.0, 101);
}

#[test]
fn a_round_trip_longer_than_the_ping_interval_still_updates_rtt_and_the_clock() {
    // A second ping went out at 1.0 s; the first pong arrives after it.
    let mut clock = Clock::at(Duration::from_millis(1200), 0);
    clock.pong(100, Duration::ZERO);
    assert_eq!(clock.rtt.rtt, Duration::from_millis(1200));
    assert_eq!(clock.tick.0, 118);
}

#[test]
fn ancient_or_future_echoes_are_ignored() {
    let mut clock = Clock::at(Duration::from_secs(10), 0);
    for sent_at in [Duration::ZERO, Duration::from_secs(11)] {
        clock.pong(100, sent_at);
    }
    assert_eq!(clock.tick.0, 0);
    assert_eq!(clock.rtt.rtt, Duration::ZERO);
    assert!(clock.rtt.measurements.is_empty());
}

#[test]
fn delayed_pong_updates_rtt_without_shifting_the_clock() {
    let mut clock = Clock::at(Duration::from_millis(200), 101);
    clock.rtt.measurements = [Duration::from_millis(10)].into();
    clock.pong(100, Duration::ZERO);
    assert_eq!(clock.tick.0, 101);
    assert_eq!(clock.rtt.rtt, Duration::from_millis(105));
}
