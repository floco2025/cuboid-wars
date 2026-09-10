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
    mut rtt: ResMut<RoundTripTime>,
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
        let now = time.elapsed();
        rtt.pending_sent_at = Some(now);
        to_server.send(ClientToServer::Send(ClientMessage::Ping(CPing {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}

pub(super) fn apply_pong(
    time: &Time,
    rtt: &mut RoundTripTime,
    sync: &mut TickSync,
    tick: &mut ServerTick,
    message: SPong,
    network: &NetworkConfig,
) {
    let Some(sent_at) = rtt.pending_sent_at else {
        return;
    };
    if message.timestamp_nanos != sent_at.as_nanos() as u64 {
        return;
    }
    let measured_rtt = time.elapsed().saturating_sub(sent_at);
    rtt.pending_sent_at = None;
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

    #[test]
    fn pong_matches_the_pending_ping_including_a_ping_sent_at_startup() {
        let mut time = Time::default();
        time.advance_by(Duration::from_millis(10));
        let mut rtt = RoundTripTime {
            pending_sent_at: Some(Duration::ZERO),
            ..default()
        };
        let mut sync = TickSync::default();
        let mut tick = ServerTick(0);
        let pong = SPong {
            tick: 100,
            timestamp_nanos: 0,
        };
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            SPong {
                timestamp_nanos: 1,
                ..pong.clone()
            },
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 0);
        assert!(rtt.pending_sent_at.is_some());
        apply_pong(
            &time,
            &mut rtt,
            &mut sync,
            &mut tick,
            pong.clone(),
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 100);
        assert_eq!(rtt.rtt, Duration::from_millis(10));
        assert!(rtt.pending_sent_at.is_none());
        tick.0 = 101;
        apply_pong(&time, &mut rtt, &mut sync, &mut tick, pong, &NetworkConfig::default());
        assert_eq!(tick.0, 101);
    }

    #[test]
    fn delayed_pong_updates_rtt_without_shifting_the_clock() {
        let mut time = Time::default();
        time.advance_by(Duration::from_millis(200));
        let mut rtt = RoundTripTime {
            pending_sent_at: Some(Duration::ZERO),
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
            SPong {
                tick: 100,
                timestamp_nanos: 0,
            },
            &NetworkConfig::default(),
        );
        assert_eq!(tick.0, 101);
        assert_eq!(rtt.rtt, Duration::from_millis(105));
    }
}
