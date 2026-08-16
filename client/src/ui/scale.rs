use bevy::{prelude::*, ui::UiScale, window::PrimaryWindow};

use crate::{config::ClientSettings, constants::HUD_MIN_SCALE};

// `None` when the window has no usable width (e.g. minimized) — keep the
// previous scale rather than collapsing the UI.
fn compute_hud_scale(window_width: f32, reference_width: f32) -> Option<f32> {
    if !window_width.is_finite() || window_width <= 0.0 {
        return None;
    }
    Some((window_width / reference_width).max(HUD_MIN_SCALE))
}

// Scale the whole screen-space HUD with the window width; the configured
// sizes are the baseline for a `hud.reference_width`-wide window. Writes
// only on change — an unconditional write would mark `UiScale` changed every
// frame and re-trigger the floating-label compensation (a full label
// relayout per frame).
pub fn ui_hud_scale_system(
    windows: Query<&Window, With<PrimaryWindow>>,
    client_settings: Res<ClientSettings>,
    mut ui_scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(scale) = compute_hud_scale(window.width(), client_settings.hud.reference_width) else {
        return;
    };
    if ui_scale.0 != scale {
        ui_scale.0 = scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_ratio_of_width_to_reference() {
        assert_eq!(compute_hud_scale(1280.0, 1280.0), Some(1.0));
        assert_eq!(compute_hud_scale(1920.0, 1280.0), Some(1.5));
        assert_eq!(compute_hud_scale(640.0, 1280.0), Some(0.5));
    }

    #[test]
    fn tiny_window_clamps_to_min_scale() {
        assert_eq!(compute_hud_scale(320.0, 1280.0), Some(HUD_MIN_SCALE));
    }

    #[test]
    fn degenerate_width_yields_no_scale() {
        assert_eq!(compute_hud_scale(0.0, 1280.0), None);
        assert_eq!(compute_hud_scale(-100.0, 1280.0), None);
        assert_eq!(compute_hud_scale(f32::NAN, 1280.0), None);
    }
}
