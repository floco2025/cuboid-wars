use anyhow::{Result, bail};

pub(super) fn validate_positive_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        return Ok(());
    }
    bail!("{path} must be positive and finite, got {value}");
}

pub(super) fn validate_non_negative_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    bail!("{path} must be non-negative and finite, got {value}");
}

pub(super) fn validate_probability(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    bail!("{path} must be between 0 and 1, got {value}");
}
