use bevy::{prelude::*, window::PrimaryWindow};
use std::time::Duration;

use crate::network::RoundTripTime;

// FPS measurement tracking.
#[derive(Resource, Default)]
pub struct FpsMeasurement {
    pub frame_count: u32,
    pub fps_timer: f32,
    pub fps: f32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_label_includes_physical_resolution() {
        assert_eq!(fps_label(59.6, Some((2560, 1440))), "FPS: 60 | 2560x1440");
        assert_eq!(fps_label(59.6, None), "FPS: 60");
    }
}
