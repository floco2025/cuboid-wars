use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{ClientSettings, settings::UserPreferences};

pub const LOCAL_SETTINGS_VERSION: u32 = 14;

// Local settings are saved after panel edits, fullscreen shortcuts, and
// window moves and resizes. The file is not in git, so a format
// change cannot reach it through `git pull`: a `version` mismatch discards the
// file (no migration), and the next save rewrites it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSettings {
    pub version: u32,
    pub fullscreen: bool,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: u32,
    pub window_height: u32,
    pub master_volume: f32,
    pub preferences: UserPreferences,
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
        let parent = path.parent().context("local settings path has no parent directory")?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("failed to create temporary settings file in {}", parent.display()))?;
        file.write_all(text.as_bytes())
            .context("failed to write temporary settings file")?;
        file.as_file()
            .sync_all()
            .context("failed to sync temporary settings file")?;
        file.persist(path)
            .map_err(|error| error.error)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        Ok(())
    }

    // The caller validates preferences after applying saved overrides.
    pub fn apply_to(&self, settings: &mut ClientSettings) {
        settings.preferences = self.preferences;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LocalSettings {
        LocalSettings {
            version: LOCAL_SETTINGS_VERSION,
            fullscreen: true,
            window_x: Some(120),
            window_y: Some(80),
            window_width: 1600,
            window_height: 900,
            master_volume: 0.8,
            preferences: UserPreferences {
                fullscreen_resolution: 1080,
                vsync: true,
                msaa_samples: 2,
                portal_view_budget: 2,
                mouse_sensitivity: 1.5,
                zoom_sensitivity: 1.5,
                invert_y: true,
                fov_degrees: 100.0,
                shake_scale: 0.5,
                show_diagnostics: false,
                rearview_mirror: true,
            },
        }
    }

    #[test]
    fn local_settings_round_trip() {
        let path = std::env::temp_dir().join(format!("cuboid_local_round_trip_{}.json", std::process::id()));
        let saved = sample();
        saved.save_to_path(&path).expect("local settings failed to save");
        let loaded = LocalSettings::load_from_path(&path).expect("saved local settings failed to load");
        assert_eq!(saved, loaded);
        let mut settings = ClientSettings::load_default().expect("client settings are invalid");
        loaded.apply_to(&mut settings);
        assert_eq!(settings.preferences, saved.preferences);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn local_settings_save_replaces_existing_file() {
        let path = std::env::temp_dir().join(format!("cuboid_local_replace_{}.json", std::process::id()));
        std::fs::write(&path, "incomplete").expect("existing settings fixture failed to write");
        let saved = sample();

        saved.save_to_path(&path).expect("local settings failed to replace");

        let loaded = LocalSettings::load_from_path(&path).expect("replaced local settings failed to load");
        assert_eq!(saved, loaded);
        let mut settings = ClientSettings::load_default().expect("client settings are invalid");
        loaded.apply_to(&mut settings);
        assert_eq!(settings.preferences, saved.preferences);
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
