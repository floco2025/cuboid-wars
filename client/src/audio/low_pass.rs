use std::{
    f32::consts::TAU,
    num::NonZero,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use bevy::{
    audio::{Decodable, Source},
    prelude::*,
};
use rodio::source::SeekError;

// The game thread writes the cutoff and the audio thread reads it per sample.
#[derive(Clone)]
pub(crate) struct LowPassCutoff(Arc<AtomicU32>);

impl LowPassCutoff {
    pub(crate) fn open() -> Self {
        Self(Arc::new(AtomicU32::new(f32::INFINITY.to_bits())))
    }

    pub(crate) fn set(&self, hz: f32) {
        self.0.store(hz.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

// A source played through a one-pole low-pass filter whose cutoff may change
// while it plays; at or beyond the Nyquist frequency the samples pass through
// untouched.
#[derive(Asset, TypePath)]
pub(crate) struct LowPassAudio<T: Asset> {
    source: T,
    cutoff: LowPassCutoff,
}

impl<T: Asset> LowPassAudio<T> {
    pub(crate) fn new(source: T, cutoff: LowPassCutoff) -> Self {
        Self { source, cutoff }
    }

    #[cfg(test)]
    pub(crate) fn cutoff(&self) -> &LowPassCutoff {
        &self.cutoff
    }
}

impl<T: Asset + Decodable> Decodable for LowPassAudio<T> {
    type Decoder = LowPassDecoder<T::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        LowPassDecoder::new(self.source.decoder(), self.cutoff.clone())
    }
}

pub(crate) struct LowPassDecoder<D> {
    input: D,
    cutoff: LowPassCutoff,
    cutoff_hz: f32,
    sample_rate: f32,
    coefficient: f32,
    state: Vec<f32>,
    channel: usize,
    span_remaining: usize,
}

impl<D: Source> LowPassDecoder<D> {
    fn new(input: D, cutoff: LowPassCutoff) -> Self {
        Self {
            input,
            cutoff,
            cutoff_hz: f32::INFINITY,
            sample_rate: 0.0,
            coefficient: 1.0,
            state: Vec::new(),
            channel: 0,
            span_remaining: 0,
        }
    }

    fn start_span(&mut self) {
        let channels = usize::from(self.input.channels().get());
        if channels != self.state.len() {
            self.state = vec![0.0; channels];
            self.channel = 0;
        }
        self.sample_rate = self.input.sample_rate().get() as f32;
        self.cutoff_hz = self.cutoff.get();
        self.coefficient = coefficient(self.cutoff_hz, self.sample_rate);
        self.span_remaining = match self.input.current_span_len() {
            Some(len) if len > 0 => len,
            _ => usize::MAX,
        };
    }
}

fn coefficient(cutoff_hz: f32, sample_rate: f32) -> f32 {
    if cutoff_hz >= sample_rate * 0.5 {
        1.0
    } else {
        1.0 - (-TAU * cutoff_hz / sample_rate).exp()
    }
}

impl<D: Source> Iterator for LowPassDecoder<D> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.span_remaining == 0 {
            self.start_span();
        }
        let sample = self.input.next()?;
        self.span_remaining -= 1;
        let cutoff_hz = self.cutoff.get();
        if cutoff_hz != self.cutoff_hz {
            self.cutoff_hz = cutoff_hz;
            self.coefficient = coefficient(cutoff_hz, self.sample_rate);
        }
        let smoothed = &mut self.state[self.channel];
        if self.coefficient >= 1.0 {
            *smoothed = sample;
        } else {
            *smoothed += self.coefficient * (sample - *smoothed);
        }
        let output = *smoothed;
        self.channel += 1;
        if self.channel == self.state.len() {
            self.channel = 0;
        }
        Some(output)
    }
}

impl<D: Source> Source for LowPassDecoder<D> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)?;
        self.span_remaining = 0;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/low_pass.rs"]
mod tests;
