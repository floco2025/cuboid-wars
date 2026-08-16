// Alpha for a "hold, then fade out" lifetime: 1.0 while `remaining_secs`
// exceeds `fade_secs`, then linear to 0.0. Shared by the HUD banner and the
// death overlay so the two fades can't drift apart.
#[must_use]
pub fn fade_out_alpha(remaining_secs: f32, fade_secs: f32) -> f32 {
    (remaining_secs / fade_secs).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_holds_then_falls_linearly() {
        assert_eq!(fade_out_alpha(5.0, 1.0), 1.0);
        assert_eq!(fade_out_alpha(0.5, 1.0), 0.5);
        assert_eq!(fade_out_alpha(0.0, 1.0), 0.0);
    }
}
