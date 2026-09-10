use std::collections::VecDeque;

use common::{config::NetworkConfig, protocol::sequence_is_newer};

use crate::{
    config::InterpolationConfig,
    constants::{
        SAMPLE_BUFFER_GAP_CAP_INTERVALS, SAMPLE_BUFFER_MAX_RATE_DEVIATION, SAMPLE_BUFFER_MAX_SAMPLES,
        SAMPLE_BUFFER_RATE_GAIN, SAMPLE_BUFFER_RECENTER_INTERVALS,
    },
};

// How far behind the newest sample playback stays and how far apart samples
// arrive, both in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleTiming {
    pub delay_ticks: f64,
    pub interval_ticks: f64,
}

impl SampleTiming {
    #[must_use]
    pub fn new(interpolation: &InterpolationConfig, network: &NetworkConfig) -> Self {
        Self {
            delay_ticks: interpolation.delay_ticks(network),
            interval_ticks: f64::from(network.server_hz) / f64::from(network.update_hz),
        }
    }
}

struct Sample<T> {
    seq: Option<u32>,
    at: f64,
    value: T,
}

// Remote samples played back a fixed lead behind the newest one. `at` is
// sender time in ticks; the cursor is playback time. The cursor runs a
// little faster or slower to keep `newest - cursor` at the target lead, so
// a stall or a latency shift does not change the lead for good.
pub struct SampleBuffer<T> {
    samples: VecDeque<Sample<T>>,
    cursor: f64,
    timing: SampleTiming,
    sealed: bool,
}

// What playback shows this frame: `left`, blended `alpha` of the way toward
// `right`. `right` is `None` while playback holds `left`: a dry buffer, a
// sample not yet due, or a seed without a sequence.
pub struct Playback<'a, T> {
    pub left: &'a T,
    pub right: Option<&'a T>,
    pub alpha: f32,
    pub span_ticks: f64,
}

impl<T> SampleBuffer<T> {
    // A seed without a sequence (a snapshot) is shown until its successor is
    // due and never blended from.
    pub fn new(seq: Option<u32>, value: T, timing: SampleTiming) -> Self {
        Self {
            samples: VecDeque::from([Sample { seq, at: 0.0, value }]),
            cursor: -timing.delay_ticks,
            timing,
            sealed: false,
        }
    }

    // Sender time of the newest sample minus playback time.
    #[cfg(test)]
    pub fn lead_ticks(&self) -> f64 {
        self.newest_at() - self.cursor
    }

    #[must_use]
    pub fn at_end(&self) -> bool {
        self.cursor >= self.newest_at()
    }

    #[must_use]
    pub fn newest(&self) -> &T {
        &self.newest_sample().value
    }

    pub fn push(&mut self, seq: u32, value: T) -> bool {
        if self.sealed {
            return false;
        }
        let last = self.newest_sample();
        let at = match last.seq {
            Some(last_seq) => {
                if !sequence_is_newer(seq, last_seq) {
                    return false;
                }
                let gap = f64::from(seq.wrapping_sub(last_seq));
                if gap > SAMPLE_BUFFER_GAP_CAP_INTERVALS * self.timing.interval_ticks {
                    // A stall or loss burst: land the sample one lead ahead of
                    // playback, so the blend runs at the target instead of a
                    // snap or a lasting lag.
                    (self.cursor + self.timing.delay_ticks).max(last.at + self.timing.interval_ticks)
                } else {
                    last.at + gap
                }
            }
            None => last.at + self.timing.interval_ticks,
        };
        self.push_at(Some(seq), at, value);
        true
    }

    // Ends the timeline `travel_ticks` after the newest sample; later samples
    // are ignored.
    pub fn seal(&mut self, travel_ticks: f64, value: T) {
        let at = self.newest_at() + travel_ticks.max(self.timing.interval_ticks);
        self.push_at(None, at, value);
        self.sealed = true;
    }

    fn push_at(&mut self, seq: Option<u32>, at: f64, value: T) {
        self.samples.push_back(Sample { seq, at, value });
        if self.samples.len() > SAMPLE_BUFFER_MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    pub fn advance(&mut self, delta_ticks: f64) -> Playback<'_, T> {
        let newest = self.newest_at();
        // The lead this frame would end with at the nominal rate, against the
        // target; a sealed timeline needs no lead and flies out to its end.
        let target = if self.sealed { 0.0 } else { self.timing.delay_ticks };
        let error = newest - (self.cursor + delta_ticks) - target;
        if !self.sealed && error > SAMPLE_BUFFER_RECENTER_INTERVALS * self.timing.interval_ticks {
            self.cursor = newest - self.timing.delay_ticks;
        } else {
            let rate = 1.0
                + (error / self.timing.interval_ticks * SAMPLE_BUFFER_RATE_GAIN)
                    .clamp(-SAMPLE_BUFFER_MAX_RATE_DEVIATION, SAMPLE_BUFFER_MAX_RATE_DEVIATION);
            self.cursor = (self.cursor + delta_ticks * rate).min(newest);
        }
        while self.samples.get(1).is_some_and(|sample| sample.at <= self.cursor) {
            self.samples.pop_front();
        }
        let left = &self.samples[0];
        let right = self.samples.get(1).filter(|_| left.seq.is_some());
        let (alpha, span_ticks) = right.map_or((0.0, 0.0), |right| {
            let span = right.at - left.at;
            (((self.cursor - left.at) / span).clamp(0.0, 1.0) as f32, span)
        });
        Playback {
            left: &left.value,
            right: right.map(|sample| &sample.value),
            alpha,
            span_ticks,
        }
    }

    fn newest_sample(&self) -> &Sample<T> {
        self.samples.back().expect("sample buffer lost its seed")
    }

    fn newest_at(&self) -> f64 {
        self.newest_sample().at
    }
}

#[cfg(test)]
#[path = "tests/sample_buffer.rs"]
mod tests;
