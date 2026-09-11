use super::*;

const LAG: Duration = Duration::from_millis(50);
const SPACING: Duration = Duration::from_millis(10);

fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

#[test]
fn delay_queue_releases_in_order_after_lag() {
    let start = Instant::now();
    let mut queue = DelayQueue::default();
    for item in 1..=3u32 {
        queue.push(start + SPACING * (item - 1), LAG, item);
    }

    assert_eq!(queue.pop_due(start + LAG - millis(1)), None);
    for expected in 1..=3u32 {
        let due = start + LAG + SPACING * (expected - 1);
        assert_eq!(queue.pop_due(due - millis(1)), None, "item {expected} released early");
        assert_eq!(queue.pop_due(due), Some(expected));
    }
    assert_eq!(queue.pop_due(start + millis(1_000)), None);
}

#[test]
fn earlier_deadlines_overtake_pending_messages() {
    let start = Instant::now();
    let mut queue = DelayQueue::default();
    queue.push(start, millis(150), "slow");
    queue.push(start + SPACING, millis(50), "quick");

    assert_eq!(queue.pop_due(start + millis(59)), None);
    assert_eq!(queue.pop_due(start + millis(60)), Some("quick"));
    assert_eq!(queue.pop_due(start + millis(149)), None);
    assert_eq!(queue.pop_due(start + millis(150)), Some("slow"));
}

#[test]
fn equal_deadlines_keep_insertion_order() {
    let start = Instant::now();
    let mut queue = DelayQueue::default();
    queue.push(start, millis(100), "slow");
    queue.push(start, millis(50), "first");
    queue.push(start, millis(50), "second");

    assert_eq!(queue.pop_due(start + millis(50)), Some("first"));
    assert_eq!(queue.pop_due(start + millis(50)), Some("second"));
    assert_eq!(queue.pop_due(start + millis(50)), None);
    assert_eq!(queue.pop_due(start + millis(100)), Some("slow"));
}

#[test]
fn jittered_unreliable_delays_never_reorder_reliable_messages() {
    let impairment = Impairment {
        lag: LAG,
        jitter: 1.0,
        ..Default::default()
    };
    let start = Instant::now();
    let mut queue = DelayQueue::default();
    for id in 0..6u32 {
        let now = start + SPACING * id;
        queue.push(now, impairment.delay(true), (id, true));
        queue.push(now, impairment.delay(false), (id, false));
    }
    let mut reliable = Vec::new();
    let mut unreliable = Vec::new();
    while let Some((id, unordered)) = queue.pop_due(start + millis(1_000)) {
        if unordered {
            unreliable.push(id);
        } else {
            reliable.push(id);
        }
    }
    assert_eq!(reliable, (0..6).collect::<Vec<_>>());
    unreliable.sort();
    assert_eq!(unreliable, reliable);
}

#[test]
fn jitter_stays_within_bounds_and_leaves_reliable_delay_fixed() {
    let impairment = Impairment {
        lag: Duration::from_millis(100),
        jitter: 0.5,
        ..Default::default()
    };
    for _ in 0..100 {
        assert!((Duration::from_millis(50)..=Duration::from_millis(150)).contains(&impairment.delay(true)));
        assert_eq!(impairment.delay(false), impairment.lag);
    }
    let fixed = Impairment {
        jitter: 0.0,
        ..impairment
    };
    assert_eq!(fixed.delay(true), fixed.lag);
}

#[test]
fn zero_lag_releases_at_once_even_with_jitter() {
    let impairment = Impairment {
        jitter: 1.0,
        ..Default::default()
    };
    assert_eq!(impairment.delay(true), Duration::ZERO);
    let now = Instant::now();
    let mut queue = DelayQueue::default();
    queue.push(now, impairment.delay(true), 7);
    assert_eq!(queue.pop_due(now), Some(7));
}

#[test]
fn drop_probability_bounds_are_exact() {
    let never = Impairment {
        drop_probability: 0.0,
        ..Default::default()
    };
    let always = Impairment {
        drop_probability: 1.0,
        ..Default::default()
    };
    for _ in 0..100 {
        assert!(!never.drops());
        assert!(always.drops());
    }
}
