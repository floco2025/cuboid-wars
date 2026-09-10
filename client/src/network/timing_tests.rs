use std::time::Duration;

use super::TickSync;
use crate::config::ClientSettings;
use common::{config::NetworkConfig, protocol::ServerTick};

#[test]
fn clock_latency_and_interpolation_delays_use_the_server_rate() {
    let settings = ClientSettings::load_default().expect("client settings invalid");
    for hz in [30, 60] {
        let network = NetworkConfig {
            server_hz: hz,
            update_hz: 10,
            snapshot_hz: 4,
        };
        let mut tick = ServerTick(0);
        TickSync::default().observe(&mut tick, 100, Duration::from_millis(200), hz);
        assert_eq!(tick.0, 100 + hz / 10);
        let seconds = settings.interpolation.delay_ticks(&network) / f64::from(hz);
        assert!((seconds - f64::from(settings.interpolation.buffer_intervals) / 10.0).abs() < 1e-6);
    }
}
