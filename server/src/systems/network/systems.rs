use bevy::prelude::*;

use super::{
    broadcast::{broadcast_to_all, broadcast_to_others, collect_items, collect_sentries, snapshot_logged_in_players},
    login::handle_login_message,
    messages::dispatch_message,
};
use crate::{
    net::ClientToServer,
    resources::{FromClientsChannel, GridConfig, ItemMap, PlayerMap, SentryMap},
};
use common::{
    constants::UPDATE_BROADCAST_INTERVAL,
    markers::{ItemMarker, PlayerMarker, SentryMarker},
    protocol::{MapLayout, *},
};

// ============================================================================
// Client Event Processing System
// ============================================================================

/// Process incoming messages from clients.
/// NOTE: Must run after `accept_connections_system` with `apply_deferred` in between,
/// otherwise entities for the messages might not be spawned yet.
pub fn network_client_message_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    grid_config: Res<GridConfig>,
    items: Res<ItemMap>,
    sentries: Res<SentryMap>,
    player_data: Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    sentry_data: Query<(&Position, &Velocity), With<SentryMarker>>,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        let Some(player_info) = players.0.get(&id) else {
            error!("received event for unknown {:?}", id);
            continue;
        };

        match event {
            ClientToServer::Disconnected => {
                let was_logged_in = player_info.logged_in;
                let entity = player_info.entity;
                players.0.remove(&id);
                commands.entity(entity).despawn();

                debug!("{:?} disconnected (logged_in: {})", id, was_logged_in);

                // Broadcast logoff to all other logged-in players if they were logged in
                if was_logged_in {
                    broadcast_to_others(&players, id, ServerMessage::Logoff(SLogoff { id, graceful: false }));
                }
            }
            ClientToServer::Message(message) => {
                let is_logged_in = player_info.logged_in;
                if is_logged_in {
                    dispatch_message(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &time,
                        &player_data,
                        &map_layout,
                    );
                } else {
                    handle_login_message(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &map_layout,
                        &grid_config,
                        &items,
                        &sentries,
                        &player_data,
                        &item_positions,
                        &sentry_data,
                    );
                }
            }
        }
    }
}

// ============================================================================
// Broadcast System
// ============================================================================

/// Broadcast authoritative game state in regular time intervals.
pub fn network_broadcast_state_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seq: Local<u32>,
    players: Res<PlayerMap>,
    items: Res<ItemMap>,
    sentries: Res<SentryMap>,
    player_data: Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    sentry_data: Query<(&Position, &Velocity), With<SentryMarker>>,
) {
    *timer += time.delta_secs();
    if *timer < UPDATE_BROADCAST_INTERVAL {
        return;
    }
    *timer = 0.0;

    // Increment sequence number
    *seq = seq.wrapping_add(1);

    if players.0.values().all(|info| !info.logged_in) {
        return; // Nothing to broadcast yet
    }

    // Collect all logged-in players
    let all_players = snapshot_logged_in_players(&players, &player_data);

    // Collect all items
    let all_items = collect_items(&items, &item_positions);

    // Collect all sentries
    let all_sentries = collect_sentries(&sentries, &sentry_data);

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate {
        seq: *seq,
        players: all_players,
        items: all_items,
        sentries: all_sentries,
    });
    //trace!("broadcasting update: {:?}", msg);
    broadcast_to_all(&players, msg);
}
