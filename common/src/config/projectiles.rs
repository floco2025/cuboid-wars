use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct ProjectilesConfig {
    pub lifetime_secs: f32,
    pub spawn_offset: f32,
    pub radius: f32,
    pub cooldown_secs: f32,
    pub gravity_scale: f32,
    pub drag_factor: f32,
    pub bounce_retention: f32,
    pub multi_shot: MultiShotConfig,
}

impl ProjectilesConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        validate_positive_finite(self.spawn_offset, &format!("{path}.spawn_offset"))?;
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.cooldown_secs, &format!("{path}.cooldown_secs"))?;
        validate_non_negative_finite(self.gravity_scale, &format!("{path}.gravity_scale"))?;
        validate_non_negative_finite(self.drag_factor, &format!("{path}.drag_factor"))?;
        if !(self.bounce_retention.is_finite() && (0.0..=1.0).contains(&self.bounce_retention)) {
            bail!("{path}.bounce_retention must be within 0.0..=1.0");
        }
        Ok(())
    }
}

const MULTI_SHOT_MAX_SHOTS: usize = 9;

// Multi-shot patterns parsed once at load. `allowed_patterns` is the ordered
// subset cycled in-game; configured but unlisted patterns stay dormant. Each
// stencil uses `x` for a shot, `.` for empty, and exactly one `o` for the aim.
#[derive(Debug, Clone, Encode, Decode, Deserialize)]
#[serde(try_from = "MultiShotSource")]
pub struct MultiShotConfig {
    allowed_patterns: Vec<String>,
    patterns: HashMap<String, MultiShotPatternConfig>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MultiShotPatternConfig {
    shots: Vec<(f32, f32)>,
}

#[derive(Deserialize)]
struct MultiShotSource {
    spread_degrees: f32,
    allowed_patterns: Vec<String>,
    patterns: HashMap<String, MultiShotPattern>,
}

#[derive(Clone, Deserialize)]
struct MultiShotPattern {
    column_scale: f32,
    row_scale: f32,
    stencil: Vec<String>,
}

impl TryFrom<MultiShotSource> for MultiShotConfig {
    type Error = anyhow::Error;

    fn try_from(source: MultiShotSource) -> Result<Self> {
        validate_positive_finite(source.spread_degrees, "multi_shot.spread_degrees")?;
        if source.allowed_patterns.is_empty() {
            bail!("multi_shot.allowed_patterns must contain at least one pattern");
        }
        let mut seen = HashSet::new();
        for name in &source.allowed_patterns {
            if !seen.insert(name) {
                bail!("multi_shot.allowed_patterns contains duplicate pattern {name:?}");
            }
            if !source.patterns.contains_key(name) {
                bail!("multi_shot.allowed_patterns contains unknown pattern {name:?}");
            }
        }

        let mut patterns = HashMap::new();
        for (name, pattern) in &source.patterns {
            let path = format!("multi_shot.patterns.{name}");
            validate_positive_finite(pattern.column_scale, &format!("{path}.column_scale"))?;
            validate_positive_finite(pattern.row_scale, &format!("{path}.row_scale"))?;
            let config = MultiShotPatternConfig::from_stencil(
                &path,
                source.spread_degrees * pattern.column_scale,
                source.spread_degrees * pattern.row_scale,
                &pattern.stencil,
            )?;
            if let Some((_, digits)) = name.rsplit_once('_')
                && let Ok(count) = digits.parse::<usize>()
                && count != config.shots().len()
            {
                bail!(
                    "{path} fires {} shots, not the {count} its name claims",
                    config.shots().len()
                );
            }
            patterns.insert(name.clone(), config);
        }
        let config = Self {
            allowed_patterns: source.allowed_patterns,
            patterns,
        };
        config.validate_pattern_count()?;
        Ok(config)
    }
}

impl MultiShotConfig {
    fn validate_pattern_count(&self) -> Result<()> {
        if self.allowed_patterns.len() > usize::from(u8::MAX) {
            bail!("multi_shot.allowed_patterns cannot contain more than 255 patterns");
        }
        Ok(())
    }

    #[must_use]
    pub fn shot_offsets(&self, pattern: u8) -> Option<&[(f32, f32)]> {
        if pattern == 0 {
            Some(&[(0.0, 0.0)])
        } else {
            self.allowed_pattern(usize::from(pattern - 1))
                .map(|(_, pattern)| pattern.shots())
        }
    }

    #[must_use]
    pub fn allowed_patterns(&self) -> &[String] {
        &self.allowed_patterns
    }

    #[must_use]
    pub fn pattern(&self, name: &str) -> Option<&MultiShotPatternConfig> {
        self.patterns
            .get(name)
            .filter(|_| self.allowed_patterns.iter().any(|allowed| allowed == name))
    }

    #[must_use]
    pub fn allowed_pattern(&self, index: usize) -> Option<(&str, &MultiShotPatternConfig)> {
        let name = self.allowed_patterns.get(index)?;
        let pattern = self
            .patterns
            .get(name)
            .expect("allowed multi-shot pattern missing after config validation");
        Some((name, pattern))
    }

    #[must_use]
    pub fn first_allowed_pattern(&self) -> (&str, &MultiShotPatternConfig) {
        self.allowed_pattern(0)
            .expect("allowed multi-shot patterns missing after config validation")
    }

    #[cfg(test)]
    pub(crate) fn from_stencil(path: &str, column_degrees: f32, row_degrees: f32, stencil: &[String]) -> Result<Self> {
        let pattern = MultiShotPatternConfig::from_stencil(path, column_degrees, row_degrees, stencil)?;
        Ok(Self {
            allowed_patterns: vec!["test".to_owned()],
            patterns: HashMap::from([("test".to_owned(), pattern)]),
        })
    }

    #[cfg(test)]
    fn shots(&self) -> &[(f32, f32)] {
        self.first_allowed_pattern().1.shots()
    }
}

impl MultiShotPatternConfig {
    fn from_stencil(path: &str, column_degrees: f32, row_degrees: f32, stencil: &[String]) -> Result<Self> {
        validate_positive_finite(column_degrees, &format!("{path}.column_degrees"))?;
        validate_positive_finite(row_degrees, &format!("{path}.row_degrees"))?;
        let Some(width) = stencil.first().map(|row| row.chars().count()) else {
            bail!("{path}.stencil must have at least one row");
        };
        if width == 0 || stencil.iter().any(|row| row.chars().count() != width) {
            bail!("{path}.stencil rows must all have the same non-zero width");
        }

        let mut cells = Vec::new();
        let mut aim = None;
        for (row, line) in stencil.iter().enumerate() {
            for (col, cell) in line.chars().enumerate() {
                let (fires, anchors) = match cell {
                    'x' => (true, false),
                    'o' => (true, true),
                    '.' => (false, false),
                    other => bail!("{path}.stencil may only contain 'x', 'o' and '.', found {other:?}"),
                };
                if anchors && aim.replace((col as f32, row as f32)).is_some() {
                    bail!("{path}.stencil may mark the aim only once");
                }
                if fires {
                    cells.push((col as f32, row as f32));
                }
            }
        }
        let Some((aim_col, aim_row)) = aim else {
            bail!("{path}.stencil must contain exactly one 'o' center shot");
        };

        let column_step = column_degrees.to_radians();
        let row_step = row_degrees.to_radians();
        let shots: Vec<(f32, f32)> = cells
            .into_iter()
            .map(|(col, row)| (-(col - aim_col) * column_step, (aim_row - row) * row_step))
            .collect();
        if shots.len() > MULTI_SHOT_MAX_SHOTS {
            bail!(
                "{path}.stencil has {} shots; max is {MULTI_SHOT_MAX_SHOTS}",
                shots.len()
            );
        }
        Ok(Self { shots })
    }

    #[must_use]
    pub fn shots(&self) -> &[(f32, f32)] {
        &self.shots
    }
}

#[cfg(test)]
#[path = "tests/projectiles.rs"]
mod tests;
