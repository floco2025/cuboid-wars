use bevy::prelude::*;
use std::time::Duration;

use crate::{cameras::CameraViewMode, network::RoundTripTime};

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

pub fn ui_fps_system(time: Res<Time>, mut fps: ResMut<FpsMeasurement>, mut query: Single<&mut Text, With<FpsMarker>>) {
    fps.frame_count += 1;
    fps.fps_timer += time.delta_secs();

    if fps.fps_timer >= 1.0 {
        fps.fps = fps.frame_count as f32 / fps.fps_timer;
        query.0 = format!("FPS: {:.0}", fps.fps);

        fps.frame_count = 0;
        fps.fps_timer = 0.0;
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
