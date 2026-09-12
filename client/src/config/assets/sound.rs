use anyhow::{Result, ensure};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SoundDef {
    pub file: String,
    #[serde(default)]
    pub volume_db: f32,
}

impl SoundDef {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        ensure!(!self.file.trim().is_empty(), "{path}.file must not be empty");
        validate_volume(self.volume_db, &format!("{path}.volume_db"))
    }
}

pub(super) fn validate_volume(value: f32, path: &str) -> Result<()> {
    ensure!(
        value.is_finite() && 10.0_f32.powf(value / 20.0).is_finite(),
        "{path} must produce a finite gain"
    );
    Ok(())
}
