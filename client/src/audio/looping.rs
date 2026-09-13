use std::{io::Cursor, num::NonZero, sync::Arc, time::Duration};

use anyhow::{Result, ensure};
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    audio::{Decodable, Source},
    prelude::*,
};
use rodio::Decoder;

#[derive(Asset, TypePath, Clone)]
pub(crate) struct LoopAudio {
    samples: Arc<[f32]>,
    channels: NonZero<u16>,
    sample_rate: NonZero<u32>,
}

impl LoopAudio {
    fn decode(bytes: Vec<u8>) -> Result<Self> {
        let decoder = Decoder::try_from(Cursor::new(bytes))?;
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();
        let samples: Arc<[f32]> = decoder.collect();
        ensure!(
            !samples.is_empty() && samples.len().is_multiple_of(usize::from(channels.get())),
            "audio loop has no complete sample frames"
        );
        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }
}

#[derive(Default, TypePath)]
pub(crate) struct LoopAudioLoader;

impl AssetLoader for LoopAudioLoader {
    type Asset = LoopAudio;
    type Settings = ();
    type Error = anyhow::Error;

    async fn load(&self, reader: &mut dyn Reader, _: &(), _: &mut LoadContext<'_>) -> Result<LoopAudio> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        LoopAudio::decode(bytes)
    }

    fn extensions(&self) -> &[&str] {
        &["wav", "ogg", "oga", "spx"]
    }
}

pub(crate) struct LoopDecoder {
    audio: LoopAudio,
    position: usize,
}

impl Decodable for LoopAudio {
    type Decoder = LoopDecoder;

    fn decoder(&self) -> LoopDecoder {
        LoopDecoder {
            audio: self.clone(),
            position: 0,
        }
    }
}

impl Iterator for LoopDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.audio.samples[self.position];
        self.position = (self.position + 1) % self.audio.samples.len();
        Some(sample)
    }
}

impl Source for LoopDecoder {
    fn current_span_len(&self) -> Option<usize> {
        // Rodio 0.22 miscounts buffered mono spans after spatialization, shifting output channels.
        None
    }

    fn channels(&self) -> NonZero<u16> {
        self.audio.channels
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.audio.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
#[path = "tests/looping.rs"]
mod tests;
