use std::{fs, path::Path, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::ClientSettings;

pub const LOCAL_SETTINGS_VERSION: u32 = 1;

// The settings panel's values, saved when the menu closes and overlaid onto
// `client.json` at startup. The file is not in git, so a format change
// cannot reach it through `git pull`: a `version` mismatch discards the file
// (no migration), and the next menu close rewrites it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSettings {
    pub version: u32,
    pub fullscreen: bool,
    pub fullscreen_resolution: u32,
    pub vsync: bool,
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    pub fov_degrees: f32,
    pub shake_scale: f32,
    pub master_volume: f32,
    pub show_diagnostics: bool,
}

fn default_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../config/client/client_local.json"
    ))
}

impl LocalSettings {
    #[must_use]
    pub fn load() -> Option<Self> {
        Self::load_from_path(&default_path())
    }

    // Runs before the log subscriber exists, so complaints go to stderr.
    fn load_from_path(path: &Path) -> Option<Self> {
        let text = fs::read_to_string(path).ok()?;
        let local: Self = match serde_json::from_str(&text) {
            Ok(local) => local,
            Err(error) => {
                eprintln!("warning: ignoring {}: {error}", path.display());
                return None;
            }
        };
        if local.version != LOCAL_SETTINGS_VERSION {
            eprintln!(
                "warning: ignoring {}: version {}, expected {}",
                path.display(),
                local.version,
                LOCAL_SETTINGS_VERSION
            );
            return None;
        }
        Some(local)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to_path(&default_path())
    }

    fn save_to_path(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self).context("failed to serialize local settings")?;
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    // Overlay onto the shipped config; the caller re-validates afterwards so
    // a hand-edited file cannot smuggle in values `client.json` couldn't.
    pub fn apply_to(&self, settings: &mut ClientSettings) {
        settings.rendering.fullscreen_resolution = self.fullscreen_resolution;
        settings.rendering.vsync = self.vsync;
        settings.input.mouse_sensitivity = self.mouse_sensitivity;
        settings.input.invert_y = self.invert_y;
        settings.camera.fov_degrees.first_person = self.fov_degrees;
        settings.camera.shake.scale = self.shake_scale;
        settings.hud.show_diagnostics = self.show_diagnostics;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LocalSettings {
        LocalSettings {
            version: LOCAL_SETTINGS_VERSION,
            fullscreen: true,
            fullscreen_resolution: 1080,
            vsync: true,
            mouse_sensitivity: 0.003,
            invert_y: true,
            fov_degrees: 100.0,
            shake_scale: 0.5,
            master_volume: 0.8,
            show_diagnostics: false,
        }
    }

    #[test]
    fn local_settings_round_trip() {
        let path = std::env::temp_dir().join(format!("cuboid_local_round_trip_{}.json", std::process::id()));
        let saved = sample();
        saved.save_to_path(&path).expect("local settings failed to save");
        let loaded = LocalSettings::load_from_path(&path).expect("saved local settings failed to load");
        assert_eq!(format!("{saved:?}"), format!("{loaded:?}"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn stale_version_is_ignored() {
        let path = std::env::temp_dir().join(format!("cuboid_local_stale_{}.json", std::process::id()));
        let mut stale = sample();
        stale.version = LOCAL_SETTINGS_VERSION + 1;
        stale.save_to_path(&path).expect("stale local settings failed to save");
        assert!(LocalSettings::load_from_path(&path).is_none());
        std::fs::remove_file(&path).ok();
    }
}
