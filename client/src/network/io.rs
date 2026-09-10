use bevy::prelude::*;
use std::time::Duration;

use super::{context::ServerMessageContext, routing::route_server_message};
use crate::{
    constants::PING_INTERVAL,
    network::{ClientToServer, ClientToServerChannel, RoundTripTime, ServerToClient, ServerToClientChannel, TickSync},
};
use common::{config::NetworkConfig, protocol::*};

// ============================================================================
// Network Message Processing System
// ============================================================================

// Main system to process all incoming messages from the server.
pub(super) fn network_receive_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut context: ServerMessageContext,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                route_server_message(message, &mut commands, &mut context);
            }
        }
    }
}

// ============================================================================
// Ping System
// ============================================================================

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
        to_server.send(ClientToServer::Send(ClientMessage::Ping(CPing {
            timestamp_nanos: time.elapsed().as_nanos() as u64,
        })));
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
mod tests {
    use super::*;

    fn pong(tick: u32, sent_at: Duration) -> SPong {
        SPong {
            tick,
            timestamp_nanos: sent_at.as_nanos() as u64,
        }
    }

    #[test]
    fn pong_measures_against_its_echoed_send_time_including_a_ping_sent_at_startup() {
        let mut time = Time::default();
        time.advance_by(Duration::from_millis(10));
        let mut rtt = RoundTripTime::default();
        let mut sync = TickSync::default();
        let mut tick = ServerTick(0);
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong(100, Duration::ZERO),
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 100);
        assert_eq!(rtt.rtt, Duration::from_millis(10));
        tick.0 = 101;
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong(100, Duration::ZERO),
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 101);
    }

    #[test]
    fn a_round_trip_longer_than_the_ping_interval_still_updates_rtt_and_the_clock() {
        let mut time = Time::default();
        time.advance_by(Duration::from_millis(1200));
        let mut rtt = RoundTripTime::default();
        let mut sync = TickSync::default();
        let mut tick = ServerTick(0);
        // A second ping went out at 1.0 s; the first pong arrives after it.
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong(100, Duration::ZERO),
            &NetworkConfig::default(),
        );
        assert_eq!(rtt.rtt, Duration::from_millis(1200));
        assert_eq!(tick.0, 118);
    }

    #[test]
    fn ancient_or_future_echoes_are_ignored() {
        let mut time = Time::default();
        time.advance_by(Duration::from_secs(10));
        let mut rtt = RoundTripTime::default();
        let mut sync = TickSync::default();
        let mut tick = ServerTick(0);
        for sent_at in [Duration::ZERO, Duration::from_secs(11)] {
            apply_pong(
                &time,
                &mut rtt,
                &mut sync,
                &mut tick,
                pong(100, sent_at),
                &NetworkConfig::default(),
            );
        }
        assert_eq!(tick.0, 0);
        assert_eq!(rtt.rtt, Duration::ZERO);
        assert!(rtt.measurements.is_empty());
    }

    #[test]
    fn delayed_pong_updates_rtt_without_shifting_the_clock() {
        let mut time = Time::default();
        time.advance_by(Duration::from_millis(200));
        let mut rtt = RoundTripTime {
            measurements: [Duration::from_millis(10)].into(),
            ..default()
        };
        let mut sync = TickSync::default();
        let mut tick = ServerTick(101);
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong(100, Duration::ZERO),
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 101);
        assert_eq!(rtt.rtt, Duration::from_millis(105));
    }
}
