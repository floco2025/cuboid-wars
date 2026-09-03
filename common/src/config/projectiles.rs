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
    pub(super) fn validate(&self, path: &str) -> Result<()> {
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
        Ok(Self {
            allowed_patterns: source.allowed_patterns,
            patterns,
        })
    }
}

impl MultiShotConfig {
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
mod tests {
    use super::*;

    fn multi_shot(stencil: &[&str]) -> Result<MultiShotConfig> {
        let rows: Vec<String> = stencil.iter().map(|row| (*row).to_owned()).collect();
        MultiShotConfig::from_stencil("multi_shot", 2.0, 3.0, &rows)
    }

    fn pattern() -> MultiShotPattern {
        MultiShotPattern {
            column_scale: 1.0,
            row_scale: 1.0,
            stencil: vec!["xo".to_owned()],
        }
    }

    #[test]
    fn selects_and_scales_a_named_pattern() {
        let patterns = HashMap::from([(
            "line".to_owned(),
            MultiShotPattern {
                column_scale: 1.5,
                ..pattern()
            },
        )]);
        let selected = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["line".to_owned()],
            patterns: patterns.clone(),
        })
        .expect("named pattern rejected");
        let (yaw, pitch) = selected.shots()[0];
        assert!((yaw - 3.0_f32.to_radians()).abs() < 1e-6 && pitch == 0.0);

        let missing = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["ring".to_owned()],
            patterns,
        });
        assert!(missing.expect_err("unknown name accepted").to_string().contains("ring"));
    }

    #[test]
    fn allowed_patterns_are_ordered_and_may_leave_dormant_patterns() {
        let patterns = HashMap::from([
            ("dormant_2".to_owned(), pattern()),
            ("second_2".to_owned(), pattern()),
            ("first_2".to_owned(), pattern()),
        ]);
        let config = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["first_2".to_owned(), "second_2".to_owned()],
            patterns: patterns.clone(),
        })
        .expect("ordered allowed patterns rejected");
        assert_eq!(config.allowed_patterns(), ["first_2", "second_2"]);
        assert!(config.pattern("dormant_2").is_none());

        let duplicate = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["first_2".to_owned(), "first_2".to_owned()],
            patterns,
        });
        assert!(
            duplicate
                .expect_err("duplicate accepted")
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn offsets_are_centred_on_the_aim() {
        let column = 2.0_f32.to_radians();
        let row = 3.0_f32.to_radians();
        assert_eq!(
            multi_shot(&["xox"]).expect("line stencil rejected").shots(),
            &[(column, 0.0), (0.0, 0.0), (-column, 0.0)]
        );
        assert_eq!(
            multi_shot(&["o", "x"]).expect("column stencil rejected").shots(),
            &[(0.0, 0.0), (0.0, -row)]
        );
    }

    #[test]
    fn triangle_is_equilateral_at_root_three_rows() {
        let rows: Vec<String> = [".o.", "x.x"].map(str::to_owned).to_vec();
        let config =
            MultiShotConfig::from_stencil("multi_shot", 1.0, 3.0_f32.sqrt(), &rows).expect("triangle stencil rejected");
        let [top, left, right] = config.shots() else {
            panic!("triangle has {} shots", config.shots().len());
        };
        let side = |a: &(f32, f32), b: &(f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        assert!((side(top, left) - side(left, right)).abs() < 1e-6);
        assert!((side(top, right) - side(left, right)).abs() < 1e-6);
    }

    #[test]
    fn anchor_moves_the_aim() {
        let column = 2.0_f32.to_radians();
        let row = 3.0_f32.to_radians();
        assert_eq!(
            multi_shot(&["x..", "..o"]).expect("anchored stencil rejected").shots(),
            &[(2.0 * column, row), (0.0, 0.0)]
        );
    }

    #[test]
    fn name_count_postfix_must_match() {
        let source = |name: &str| MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec![name.to_owned()],
            patterns: HashMap::from([(name.to_owned(), pattern())]),
        };
        assert!(MultiShotConfig::try_from(source("line_2")).is_ok());
        assert!(MultiShotConfig::try_from(source("line")).is_ok());
        let error = MultiShotConfig::try_from(source("line_3")).expect_err("wrong count accepted");
        assert!(error.to_string().contains("not the 3"));
    }

    #[test]
    fn pattern_is_validated() {
        let error = |pattern: &[&str]| multi_shot(pattern).expect_err("invalid stencil accepted").to_string();
        assert!(error(&["xx", "x"]).contains("width"));
        assert!(error(&["x-x"]).contains("'x', 'o' and '.'"));
        assert!(error(&["..."]).contains("center shot"));
        assert!(error(&["xxx"]).contains("center shot"));
        assert!(error(&["o.o"]).contains("only once"));
        assert!(error(&["oxxxxxxxxx"]).contains("max is"));
        assert!(multi_shot(&["x.x", ".o.", "x.x"]).is_ok());
    }
}
