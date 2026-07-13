use bevy::{prelude::*, ui::UiScale, window::PrimaryWindow};
use std::time::Duration;

use crate::{cameras::CameraViewMode, config::ClientSettings, constants::HUD_MIN_SCALE, network::RoundTripTime};

// FPS measurement tracking.
#[derive(Resource, Default)]
pub struct FpsMeasurement {
    pub frame_count: u32,
    pub fps_timer: f32,
    pub fps: f32,
}

// Marker for the crosshair UI node (visible in first-person view only).
#[derive(Component)]
pub struct CrosshairMarker;

// Marker for the RTT (round-trip time) text node in the HUD.
#[derive(Component)]
pub struct RttMarker;

// Marker for the FPS counter text node in the HUD.
#[derive(Component)]
pub struct FpsMarker;

pub fn ui_rtt_system(rtt: Res<RoundTripTime>, mut query: Single<&mut Text, With<RttMarker>>) {
    if !rtt.is_changed() {
        return;
    }

    if rtt.rtt > Duration::ZERO {
        query.0 = format!("RTT: {:.0}ms", rtt.rtt.as_secs_f64() * 1000.0);
    } else {
        query.0 = "RTT: --".to_string();
    }
}

pub fn ui_fps_system(
    time: Res<Time>,
    mut fps: ResMut<FpsMeasurement>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut query: Single<&mut Text, With<FpsMarker>>,
) {
    fps.frame_count += 1;
    fps.fps_timer += time.delta_secs();

    if fps.fps_timer >= 1.0 {
        fps.fps = fps.frame_count as f32 / fps.fps_timer;
        let resolution = windows
            .single()
            .ok()
            .map(|window| (window.physical_width(), window.physical_height()));
        query.0 = fps_label(fps.fps, resolution);

        fps.frame_count = 0;
        fps.fps_timer = 0.0;
    }
}

fn fps_label(fps: f32, resolution: Option<(u32, u32)>) -> String {
    resolution.map_or_else(
        || format!("FPS: {fps:.0}"),
        |(width, height)| format!("FPS: {fps:.0} | {width}x{height}"),
    )
}

// Alpha for a "hold, then fade out" lifetime: 1.0 while `remaining_secs`
// exceeds `fade_secs`, then linear to 0.0. Shared by the HUD banner and the
// death overlay so the two fades can't drift apart.
#[must_use]
pub fn fade_out_alpha(remaining_secs: f32, fade_secs: f32) -> f32 {
    (remaining_secs / fade_secs).clamp(0.0, 1.0)
}

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

pub fn ui_crosshair_visibility_system(
    view_mode: Res<CameraViewMode>,
    mut query: Query<&mut Visibility, With<CrosshairMarker>>,
) {
    if !view_mode.is_changed() {
        return;
    }

    for mut visibility in &mut query {
        *visibility = if view_mode.is_first_person() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
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

    #[test]
    fn fade_holds_then_falls_linearly() {
        assert_eq!(fade_out_alpha(5.0, 1.0), 1.0);
        assert_eq!(fade_out_alpha(0.5, 1.0), 0.5);
        assert_eq!(fade_out_alpha(0.0, 1.0), 0.0);
    }

    #[test]
    fn fps_label_includes_physical_resolution() {
        assert_eq!(fps_label(59.6, Some((2560, 1440))), "FPS: 60 | 2560x1440");
        assert_eq!(fps_label(59.6, None), "FPS: 60");
    }
}
