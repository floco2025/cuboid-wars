use bevy::prelude::*;
use std::time::Duration;

use crate::{
    markers::{CrosshairUIMarker, FpsUIMarker, RttUIMarker},
    resources::{CameraViewMode, FpsMeasurement, RoundTripTime},
};

pub fn ui_rtt_system(rtt: Res<RoundTripTime>, mut query: Single<&mut Text, With<RttUIMarker>>) {
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
    mut query: Single<&mut Text, With<FpsUIMarker>>,
) {
    fps.frame_count += 1;
    fps.fps_timer += time.delta_secs();

    if fps.fps_timer >= 1.0 {
        fps.fps = fps.frame_count as f32 / fps.fps_timer;
        query.0 = format!("FPS: {:.0}", fps.fps);

        fps.frame_count = 0;
        fps.fps_timer = 0.0;
    }
}

pub fn ui_toggle_crosshair_system(
    view_mode: Res<CameraViewMode>,
    mut query: Query<&mut Visibility, With<CrosshairUIMarker>>,
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
