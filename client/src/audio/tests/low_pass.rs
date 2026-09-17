use std::{f32::consts::TAU, num::NonZero, time::Duration};

use bevy::audio::Source;

use super::*;

struct TestSource {
    samples: Vec<f32>,
    position: usize,
    channels: u16,
    sample_rate: u32,
    span: usize,
}

impl Iterator for TestSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = *self.samples.get(self.position)?;
        self.position += 1;
        Some(sample)
    }
}

impl Source for TestSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.span)
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::new(self.channels).expect("zero channels")
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).expect("zero sample rate")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

// Left: an 8 kHz tone at 48 kHz; right: a constant.
fn stereo_frames(frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|frame| [(TAU * 8000.0 * frame as f32 / 48000.0).sin(), 0.5])
        .collect()
}

fn decoder(samples: Vec<f32>, cutoff: &LowPassCutoff) -> LowPassDecoder<TestSource> {
    LowPassDecoder::new(
        TestSource {
            samples,
            position: 0,
            channels: 2,
            sample_rate: 48000,
            span: 7,
        },
        cutoff.clone(),
    )
}

#[test]
fn an_open_cutoff_passes_every_sample_through_unchanged() {
    let samples = stereo_frames(300);
    let output: Vec<f32> = decoder(samples.clone(), &LowPassCutoff::open()).collect();
    assert_eq!(output, samples);
}

#[test]
fn a_low_cutoff_smooths_each_channel_separately_and_follows_live_changes() {
    let samples = stereo_frames(4800);
    let cutoff = LowPassCutoff::open();
    cutoff.set(500.0);
    let mut decoder = decoder(samples.clone(), &cutoff);
    assert_eq!(decoder.channels().get(), 2);
    let muffled: Vec<f32> = decoder.by_ref().take(4800).collect();
    let settled = &muffled[2400..];
    let tone_peak = settled
        .iter()
        .step_by(2)
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(tone_peak < 0.1, "8 kHz tone left at {tone_peak}");
    assert!(
        settled
            .iter()
            .skip(1)
            .step_by(2)
            .all(|sample| (sample - 0.5).abs() < 1e-3)
    );
    cutoff.set(f32::INFINITY);
    let passed: Vec<f32> = decoder.collect();
    assert_eq!(passed, samples[4800..]);
}
