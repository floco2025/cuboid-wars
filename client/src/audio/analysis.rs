use std::{collections::HashMap, fs};

use anyhow::{Context, Result, ensure};
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Resource, Deserialize)]
pub(crate) struct AudioAnalysis {
    version: u32,
    sounds: HashMap<String, SoundAnalysis>,
}

#[derive(Deserialize)]
struct SoundAnalysis {
    suggested_gain_db: f32,
}

impl AudioAnalysis {
    pub(crate) fn load_default() -> Result<Self> {
        let text = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/sounds/analysis.json"))
            .context("failed to read sounds/analysis.json")?;
        let analysis: Self = serde_json::from_str(&text).context("failed to parse sounds/analysis.json")?;
        ensure!(analysis.version == 1, "unsupported audio analysis version");
        for (path, sound) in &analysis.sounds {
            ensure!(
                sound.suggested_gain_db.is_finite() && sound.suggested_gain_db <= 0.0,
                "audio analysis gain for {path} must be finite and nonpositive"
            );
        }
        Ok(analysis)
    }

    pub(crate) fn gain(&self, path: &str) -> f32 {
        self.sounds
            .get(path)
            .map_or(1.0, |sound| 10.0_f32.powf(sound.suggested_gain_db / 20.0))
    }
}
