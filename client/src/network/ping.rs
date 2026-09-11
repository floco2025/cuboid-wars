use bevy::prelude::*;
use std::time::Duration;

use crate::{
    constants::PING_INTERVAL,
    network::{ClientToServerChannel, RoundTripTime, TickSync},
};
use common::{config::NetworkConfig, protocol::*};

// System to send ping requests every `PING_INTERVAL` seconds.
pub(super) fn network_ping_system(
    time: Res<Time>,
    to_server: Res<ClientToServerChannel>,
    mut timer: Local<f32>,
    mut initialized: Local<bool>,
) {
    if !*initialized {
        *timer = PING_INTERVAL;
        *initialized = true;
    }

    let delta = time.delta_secs();
    *timer += delta;

    // Send ping request every PING_INTERVAL seconds
    if *timer >= PING_INTERVAL {
        *timer = 0.0;
        to_server.send(ClientMessage::Ping(CPing {
            timestamp_nanos: time.elapsed().as_nanos() as u64,
        }));
    }
}

// Older echoes than this are forged or from before a clock reset and must not feed the RTT or the clock.
const PONG_MAX_AGE: Duration = Duration::from_secs(5);

// Every pong measures against the send time it echoes, so a round trip longer
// than the ping interval still counts.
pub(super) fn apply_pong(
    time: &Time,
    rtt: &mut RoundTripTime,
    sync: &mut TickSync,
    tick: &mut ServerTick,
    message: SPong,
    network: &NetworkConfig,
) {
    let sent_at = Duration::from_nanos(message.timestamp_nanos);
    let Some(measured_rtt) = time.elapsed().checked_sub(sent_at).filter(|rtt| *rtt <= PONG_MAX_AGE) else {
        return;
    };
    rtt.measurements.push_back(measured_rtt);
    if rtt.measurements.len() > 10 {
        rtt.measurements.pop_front();
    }
    let sum: Duration = rtt.measurements.iter().sum();
    rtt.rtt = sum / rtt.measurements.len() as u32;
    let fastest = rtt.measurements.iter().copied().min().unwrap_or(measured_rtt);
    // A delayed pong raises the RTT display without pulling the world clock along with it.
    if measured_rtt <= fastest + network.tick_duration() {
        sync.observe(tick, message.tick, measured_rtt, network.server_hz);
    }
}

#[cfg(test)]
#[path = "tests/ping.rs"]
mod tests;
