use bevy::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{ItemMap, PlayerMap, SentryMap},
};
use common::{
    constants::*,
    markers::{ItemMarker, PlayerMarker, SentryMarker},
    protocol::*,
};

// ============================================================================
// Broadcasting Helpers
// ============================================================================

// Broadcast `message` to every logged-in player except `skip`.
pub fn broadcast_to_others(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in &players.0 {
        if *other_id != skip && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

// Broadcast `message` to every logged-in player.
pub fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.0.values() {
        if player_info.logged_in {
            let _ = player_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

// ============================================================================
// Data Collection Functions
// ============================================================================

// Collect all logged-in players for network updates.
#[must_use]
pub fn snapshot_logged_in_players(
    players: &PlayerMap,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
) -> Vec<(PlayerId, Player)> {
    players
        .0
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let (pos, speed, face_dir) = player_data.get(info.entity).ok()?;
            Some((
                *player_id,
                Player {
                    name: info.name.clone(),
                    pos: *pos,
                    speed: *speed,
                    face_dir: face_dir.0,
                    hits: info.hits,
                    speed_power_up: ALWAYS_SPEED || info.speed_power_up_timer > 0.0,
                    multi_shot_power_up: ALWAYS_MULTI_SHOT || info.multi_shot_power_up_timer > 0.0,
                    phasing_power_up: ALWAYS_PHASING || info.phasing_power_up_timer > 0.0,
                    sentry_hunt_power_up: ALWAYS_SENTRY_HUNT || info.sentry_hunt_power_up_timer > 0.0,
                    stunned: info.stun_timer > 0.0,
                },
            ))
        })
        .collect()
}

// Build the authoritative item list that gets replicated to clients.
#[must_use]
pub fn collect_items(items: &ItemMap, item_positions: &Query<&Position, With<ItemMarker>>) -> Vec<(ItemId, Item)> {
    items
        .0
        .iter()
        .filter(|(_, info)| {
            // Filter out cookies that are currently respawning (spawn_time > 0)
            info.item_type != ItemType::Cookie || info.spawn_time == 0.0
        })
        .map(|(id, info)| {
            let pos_component = item_positions.get(info.entity).expect("Item entity missing Position");
            (
                *id,
                Item {
                    item_type: info.item_type,
                    pos: *pos_component,
                },
            )
        })
        .collect()
}

// Build the authoritative sentry list that gets replicated to clients.
#[must_use]
pub fn collect_sentries(
    sentries: &SentryMap,
    sentry_data: &Query<(&Position, &Velocity), With<SentryMarker>>,
) -> Vec<(SentryId, Sentry)> {
    sentries
        .0
        .iter()
        .map(|(id, info)| {
            let (pos_component, vel_component) =
                sentry_data.get(info.entity).expect("Sentry entity missing components");
            (
                *id,
                Sentry {
                    pos: *pos_component,
                    vel: *vel_component,
                },
            )
        })
        .collect()
}
