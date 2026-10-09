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
