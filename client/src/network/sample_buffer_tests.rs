use super::{SampleBuffer, SampleTiming};

const DELAY: f64 = 2.0;

fn timing(interval_ticks: f64) -> SampleTiming {
    SampleTiming {
        delay_ticks: DELAY,
        interval_ticks,
    }
}

fn shown(buffer: &mut SampleBuffer<f64>, delta_ticks: f64) -> f64 {
    let playback = buffer.advance(delta_ticks);
    playback.right.map_or(*playback.left, |right| {
        left_to_right(*playback.left, *right, playback.alpha)
    })
}

fn left_to_right(left: f64, right: f64, alpha: f32) -> f64 {
    left + (right - left) * f64::from(alpha)
}

// Sender steps once per tick and its sample for step `k` lands `latency` ticks later.
fn stream(buffer: &mut SampleBuffer<f64>, ticks: std::ops::Range<u32>, latency: u32, send: impl Fn(u32) -> bool) {
    for tick in ticks {
        if let Some(step) = tick.checked_sub(latency)
            && step > 0
            && send(step)
        {
            buffer.push(step, f64::from(step));
        }
        buffer.advance(1.0);
    }
}

#[test]
fn steady_stream_plays_the_configured_lead_behind_the_newest_sample() {
    let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
    let mut previous = -1.0;
    for step in 1..200 {
        buffer.push(step, f64::from(step));
        let value = shown(&mut buffer, 1.0);
        assert!(value >= previous, "playback rewound at step {step}");
        previous = value;
        if step > 80 {
            assert!(
                (buffer.lead_ticks() - DELAY).abs() < 0.01,
                "lead drifted at step {step}"
            );
            assert!((value - (f64::from(step) - DELAY)).abs() < 0.01);
        }
    }
}

#[test]
fn an_unordered_seed_holds_until_its_successor_is_due_and_is_never_blended_from() {
    let mut buffer = SampleBuffer::new(None, 10.0, timing(1.0));
    let mut seed_frames = 0;
    for step in 7..20 {
        buffer.push(step, f64::from(step) * 10.0);
        let playback = buffer.advance(1.0);
        if *playback.left == 10.0 {
            assert!(playback.right.is_none(), "playback blended out of the seed");
            seed_frames += 1;
        } else {
            let right = *playback.right.expect("ordered samples must blend");
            assert!(*playback.left >= 70.0 && right > *playback.left);
        }
    }
    assert!((1..=4).contains(&seed_frames), "seed shown for {seed_frames} frames");
}

#[test]
fn repeated_and_late_sequences_are_ignored_across_the_wrap() {
    let mut buffer = SampleBuffer::new(Some(u32::MAX - 1), 0.0, timing(1.0));
    assert!(buffer.push(1, 3.0));
    assert!(!buffer.push(u32::MAX, -10.0));
    assert!(!buffer.push(1, -10.0));
    assert_eq!(*buffer.newest(), 3.0);
    let mut previous = 0.0;
    for _ in 0..10 {
        let value = shown(&mut buffer, 0.5);
        assert!(value >= previous);
        previous = value;
    }
    assert_eq!(previous, 3.0);
}

#[test]
fn playback_never_passes_the_newest_sample() {
    let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
    buffer.push(1, 1.0);
    for _ in 0..50 {
        assert!(shown(&mut buffer, 1.0) <= 1.0);
    }
    assert_eq!(shown(&mut buffer, 1.0), 1.0);
    assert!(buffer.at_end());
    assert!(buffer.lead_ticks() >= 0.0);
}

// The lead seen over one sample interval; samples land in steps of an interval, so a single frame is off by up to that.
fn mean_lead(
    buffer: &mut SampleBuffer<f64>,
    ticks: std::ops::Range<u32>,
    latency: u32,
    send: impl Fn(u32) -> bool,
) -> f64 {
    let mut total = 0.0;
    for tick in ticks.clone() {
        stream(buffer, tick..tick + 1, latency, &send);
        total += buffer.lead_ticks();
    }
    total / f64::from(ticks.len() as u32)
}

#[test]
fn the_lead_returns_to_its_target_after_a_stall() {
    for interval in [1.0, 3.0] {
        let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(interval));
        let cadence = |step: u32| step.is_multiple_of(interval as u32);
        stream(&mut buffer, 1..100, 2, cadence);
        stream(&mut buffer, 100..130, 2, |step| {
            cadence(step) && !(95..125).contains(&step)
        });
        let mut previous = shown(&mut buffer, 0.0);
        for tick in 130..400 {
            stream(&mut buffer, tick..tick + 1, 2, cadence);
            let value = shown(&mut buffer, 0.0);
            assert!(value >= previous, "playback rewound at tick {tick}");
            previous = value;
        }
        let lead = mean_lead(&mut buffer, 400..400 + interval as u32, 2, cadence);
        assert!(
            (lead - DELAY).abs() < 0.25,
            "lead {lead} after a stall at interval {interval}"
        );
    }
}

#[test]
fn the_lead_returns_to_its_target_after_a_latency_shift() {
    for (before, after) in [(2, 6), (6, 2)] {
        let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
        stream(&mut buffer, 1..200, before, |_| true);
        assert!((buffer.lead_ticks() - DELAY).abs() < 0.25);
        stream(&mut buffer, 200..320, after, |_| true);
        assert!(
            (buffer.lead_ticks() - DELAY).abs() < 0.25,
            "lead {} after shifting latency from {before} to {after}",
            buffer.lead_ticks()
        );
    }
}

#[test]
fn a_sequence_jump_lands_one_lead_ahead_and_blends_there() {
    let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
    stream(&mut buffer, 1..50, 2, |_| true);
    let before = shown(&mut buffer, 0.0);
    buffer.push(1_000_000, 100.0);
    let lead = buffer.lead_ticks();
    assert!(
        (DELAY - 1e-6..=DELAY + 1.0 + 1e-6).contains(&lead),
        "lead {lead} after the jump"
    );
    let mut previous = before;
    let mut frames = 0;
    while previous < 100.0 {
        let value = shown(&mut buffer, 1.0);
        assert!(value >= previous && value <= 100.0);
        previous = value;
        frames += 1;
        assert!(frames <= 10, "playback never reached the jumped sample");
    }
    assert!(frames >= 3, "playback snapped to the jumped sample");
}

#[test]
fn a_lead_far_past_the_target_is_recentred_at_once() {
    let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
    for step in 1..=8 {
        buffer.push(step, f64::from(step));
    }
    let value = shown(&mut buffer, 1.0);
    assert!((buffer.lead_ticks() - DELAY).abs() < 1e-6);
    assert!((value - (8.0 - DELAY)).abs() < 1e-6);
}

#[test]
fn sealing_lands_the_end_point_after_its_travel_and_ignores_later_samples() {
    let mut buffer = SampleBuffer::new(Some(0), 0.0, timing(1.0));
    buffer.push(1, 1.0);
    buffer.seal(3.0, 4.0);
    assert!(!buffer.push(2, 50.0));
    let mut previous = shown(&mut buffer, 0.0);
    while !buffer.at_end() {
        let value = shown(&mut buffer, 0.5);
        assert!(value >= previous && value <= 4.0);
        previous = value;
    }
    assert_eq!(previous, 4.0);
}
