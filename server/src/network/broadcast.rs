use bevy::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{ActorMap, ItemMap, PlayerMap},
};
use common::{physics::CharacterVerticalVelocity, protocol::*};

// ============================================================================
// Broadcasting Helpers
// ============================================================================

// Broadcast `message` to every logged-in player except `skip`.
pub fn broadcast_to_others(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in players.iter() {
        if *other_id != skip && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

// Broadcast `message` to every logged-in player.
pub fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.values() {
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
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
) -> Vec<(PlayerId, Player)> {
    players
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let (pos, move_intent, face_dir, health) = player_data.get(info.entity).ok()?;
            let vertical_velocity = motions.get(info.entity).map_or(0.0, |m| m.0);
            Some((
                *player_id,
                Player {
                    name: info.name.clone(),
                    movement: PlayerMovementState::new(*pos, *move_intent, vertical_velocity),
                    face_dir: face_dir.0,
                    health: *health,
                    hits: info.hits,
                    speed_power_up: info.has_speed(),
                    multi_shot_power_up: info.has_multi_shot(),
                    phasing_power_up: info.has_phasing(),
                    anti_gravity_power_up: info.has_anti_gravity(),
                    stunned: info.stun_timer > 0.0,
                },
            ))
        })
        .collect()
}

// Collect all server-controlled actors for network updates.
#[must_use]
pub fn snapshot_actors(
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<ActorMarker>>,
) -> Vec<(ActorId, Actor)> {
    actors
        .iter()
        .filter_map(|(actor_id, info)| {
            let (pos, move_intent, face_dir, health) = actor_data.get(info.entity).ok()?;
            let vertical_velocity = motions.get(info.entity).map_or(0.0, |m| m.0);
            Some((
                *actor_id,
                Actor {
                    kind: info.spawn_kind.clone(),
                    movement: ActorMovementState::new(*pos, *move_intent, vertical_velocity),
                    face_dir: face_dir.0,
                    health: *health,
                },
            ))
        })
        .collect()
}

// Build the authoritative item list that gets replicated to clients.
#[must_use]
pub fn collect_items(items: &ItemMap, item_positions: &Query<&Position, With<ItemMarker>>) -> Vec<(ItemId, Item)> {
    items
        .iter()
        .filter(|(_, info)| {
            // Filter out cookies and keys that are currently respawning
            // (spawn_time > 0) — their entities exist but should be invisible
            // to clients until the timer elapses.
            !matches!(info.item_type, ItemType::Cookie | ItemType::Key(_)) || info.spawn_time == 0.0
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
