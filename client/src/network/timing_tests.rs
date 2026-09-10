use std::time::Duration;

use super::TickSync;
use crate::config::InterpolationConfig;
use common::{config::NetworkConfig, protocol::ServerTick};

#[test]
fn clock_latency_compensation_uses_the_server_rate() {
    for hz in [30, 60] {
        let mut tick = ServerTick(0);
        TickSync::default().observe(&mut tick, 100, Duration::from_millis(200), hz);
        assert_eq!(tick.0, 100 + hz / 10);
    }
}

#[test]
fn interpolation_delay_spans_the_configured_update_intervals_at_any_server_rate() {
    let interpolation = InterpolationConfig { buffer_intervals: 2.0 };
    for hz in [30, 60] {
        let network = NetworkConfig {
            server_hz: hz,
            update_hz: 10,
            snapshot_hz: 4,
        };
        let seconds = interpolation.delay_ticks(&network) / f64::from(hz);
        assert!((seconds - 0.2).abs() < 1e-6);
    }
}
