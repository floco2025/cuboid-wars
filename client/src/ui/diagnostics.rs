use bevy::prelude::*;
use std::time::Duration;

use crate::{cameras::SceneRenderTarget, config::ClientSettings, network::RoundTripTime};

// FPS measurement tracking.
#[derive(Resource, Default)]
pub struct FpsMeasurement {
    pub frame_count: u32,
    pub fps_timer: f32,
    pub fps: f32,
}

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
    let target = if client_settings.hud.show_diagnostics {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
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
    time: Res<Time>,
    mut fps: ResMut<FpsMeasurement>,
    scene_target: Res<SceneRenderTarget>,
    mut query: Single<&mut Text, With<FpsMarker>>,
) {
    fps.frame_count += 1;
    fps.fps_timer += time.delta_secs();

    if fps.fps_timer >= 1.0 {
        fps.fps = fps.frame_count as f32 / fps.fps_timer;
        query.0 = fps_label(fps.fps, scene_target.size);

        fps.frame_count = 0;
        fps.fps_timer = 0.0;
    }
}

fn fps_label(fps: f32, render_size: UVec2) -> String {
    format!("FPS: {fps:.0} | {}x{}", render_size.x, render_size.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_label_includes_render_size() {
        assert_eq!(fps_label(59.6, UVec2::new(2560, 1440)), "FPS: 60 | 2560x1440");
    }
}
