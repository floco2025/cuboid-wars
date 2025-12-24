use bevy::prelude::*;
use std::time::Duration;

use super::{components::AssetManagers, login::handle_init_message, messages::dispatch_message};
use crate::{
    constants::ECHO_INTERVAL,
    markers::MainCameraMarker,
    net::{ClientToServer, ServerToClient},
    resources::{
        ClientToServerChannel, ItemMap, LastUpdateSeq, MyPlayerId, PlayerMap, RoundTripTime, SentryMap,
        ServerToClientChannel,
    },
};
use common::{markers::PlayerMarker, markers::SentryMarker, protocol::*};

// ============================================================================
// Network Message Processing System
// ============================================================================

// Main system to process all incoming messages from the server.
pub fn network_server_message_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    mut sentries: ResMut<SentryMap>,
    mut rtt: ResMut<RoundTripTime>,
    mut last_update_seq: ResMut<LastUpdateSeq>,
    mut assets: AssetManagers,
    player_data: Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    sentry_positions: Query<&Position, With<SentryMarker>>,
    cameras: Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: Option<Res<MyPlayerId>>,
    map_layout: Option<Res<MapLayout>>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                if let Some(my_id) = my_player_id.as_ref() {
                    dispatch_message(
                        message,
                        my_id.0,
                        &mut commands,
                        &mut players,
                        &mut items,
                        &mut sentries,
                        &mut rtt,
                        &mut last_update_seq,
                        &mut assets,
                        &player_data,
                        &sentry_positions,
                        &cameras,
                        &time,
                        &asset_server,
                        map_layout.as_deref(),
                    );
                } else {
                    handle_init_message(message, &mut commands);
                }
            }
        }
    }
}

// ============================================================================
// Echo/Ping System
// ============================================================================

// System to send echo requests every `ECHO_INTERVAL` seconds.
pub fn network_echo_system(
    time: Res<Time>,
    mut rtt: ResMut<RoundTripTime>,
    to_server: Res<ClientToServerChannel>,
    mut timer: Local<f32>,
    mut initialized: Local<bool>,
) {
    // Initialize timer to send first echo after 1 second
    if !*initialized {
        *timer = ECHO_INTERVAL - 1.0;
        *initialized = true;
    }

    let delta = time.delta_secs();
    *timer += delta;

    // Send echo request every ECHO_INTERVAL seconds
    if *timer >= ECHO_INTERVAL {
        *timer = 0.0;
        let now = time.elapsed();
        rtt.pending_sent_at = now;
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Echo(CEcho {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}

// Handle echo response from server to calculate RTT.
pub fn handle_echo_message(time: &Res<Time>, rtt: &mut ResMut<RoundTripTime>, msg: SEcho) {
    if rtt.pending_sent_at == Duration::ZERO {
        return;
    }

    let expected_nanos = rtt.pending_sent_at.as_nanos() as u64;
    if msg.timestamp_nanos != expected_nanos {
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
