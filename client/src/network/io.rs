use bevy::prelude::*;
use std::time::Duration;

use super::{context::ServerMessageContext, routing::route_server_message};
use crate::{
    constants::PING_INTERVAL,
    network::{ClientToServer, ClientToServerChannel, RoundTripTime, ServerToClient, ServerToClientChannel},
};
use common::protocol::*;

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
    // Initialize timer to send first ping after 1 second
    if !*initialized {
        *timer = PING_INTERVAL - 1.0;
        *initialized = true;
    }

    let delta = time.delta_secs();
    *timer += delta;

    // Send ping request every PING_INTERVAL seconds
    if *timer >= PING_INTERVAL {
        *timer = 0.0;
        let now = time.elapsed();
        rtt.pending_sent_at = now;
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Ping(CPing {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}

// Handle pong response from server to calculate RTT.
pub(super) fn apply_pong(time: &Time, rtt: &mut RoundTripTime, message: SPong) {
    if rtt.pending_sent_at == Duration::ZERO {
        return;
    }

    let expected_nanos = rtt.pending_sent_at.as_nanos() as u64;
    if message.timestamp_nanos != expected_nanos {
        return;
    }

    let now = time.elapsed();
    let measured_rtt = now - rtt.pending_sent_at;
    rtt.pending_sent_at = Duration::ZERO;

    rtt.measurements.push_back(measured_rtt);
    if rtt.measurements.len() > 10 {
        rtt.measurements.pop_front();
    }

    let sum: Duration = rtt.measurements.iter().sum();
    rtt.rtt = sum / rtt.measurements.len() as u32;
}
