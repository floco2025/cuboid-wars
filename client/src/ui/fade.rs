// Alpha for a "hold, then fade out" lifetime: 1.0 while `remaining_secs`
// exceeds `fade_secs`, then linear to 0.0. Shared by the HUD banner and the
// death overlay so the two fades can't drift apart.
#[must_use]
pub fn fade_out_alpha(remaining_secs: f32, fade_secs: f32) -> f32 {
    if fade_secs == 0.0 {
        return if remaining_secs > 0.0 { 1.0 } else { 0.0 };
    }
    (remaining_secs / fade_secs).clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "tests/fade.rs"]
mod tests;
