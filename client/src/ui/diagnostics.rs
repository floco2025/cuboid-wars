use bevy::{
    diagnostic::{Diagnostic, DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use std::time::Duration;

use crate::{cameras::SceneRenderTarget, config::ClientSettings, network::RoundTripTime};

// Marker for the column holding the RTT and FPS readouts.
#[derive(Component)]
pub struct DiagnosticsColumnMarker;

pub fn ui_diagnostics_visibility_system(
    client_settings: Res<ClientSettings>,
    mut visibility: Single<&mut Visibility, With<DiagnosticsColumnMarker>>,
) {
    if !client_settings.is_changed() {
        return;
    }
    let target = if client_settings.preferences.show_diagnostics {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    // Unrelated settings changes should not retrigger visibility propagation.
    visibility.set_if_neq(target);
}

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
    diagnostics: Res<DiagnosticsStore>,
    scene_target: Res<SceneRenderTarget>,
    mut query: Single<&mut Text, With<FpsMarker>>,
) {
    // The mean frame time over the diagnostic's history, so alternating fast
    // and slow frames report the sustained rate. `smoothed` would not: its
    // default time constant is under one frame at 60 FPS.
    let Some(frame_time_ms) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(Diagnostic::average)
        .filter(|frame_time_ms| *frame_time_ms > 0.0)
    else {
        return;
    };
    query.0 = fps_label((1000.0 / frame_time_ms) as f32, scene_target.size);
}

fn fps_label(fps: f32, render_size: UVec2) -> String {
    format!("FPS: {fps:.0} | {}x{}", render_size.x, render_size.y)
}
