use bevy::prelude::*;

use crate::{
    actors::{ActorInfo, ActorMap, ActorMotionQuery, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    missiles::MissileMap,
    players::{PlayerInfo, PlayerMap, PlayerStateQuery},
    portals::PortalAssignments,
};
use common::{map::Carriers, protocol::*};

// ============================================================================
// Broadcasting Helpers
// ============================================================================

// Broadcast `message` to every active player except `skip`.
pub fn broadcast_to_others(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in players.iter() {
        if *other_id != skip && other_info.connection.logged_in {
            let _ = other_info.connection.channel.send(message.clone());
        }
    }
}

// Broadcast `message` to every active player.
pub fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.values() {
        if player_info.connection.logged_in {
            let _ = player_info.connection.channel.send(message.clone());
        }
    }
}

// Each client owns its movement, so a recipient never gets its own entry back.
pub(super) fn broadcast_player_moves(players: &PlayerMap, tick: u32, moves: &[PlayerMove]) {
    for (recipient, info) in players.iter() {
        if !info.connection.logged_in {
            continue;
        }
        let moves: Vec<PlayerMove> = moves.iter().filter(|entry| entry.id != *recipient).copied().collect();
        if moves.is_empty() {
            continue;
        }
        let _ = info
            .connection
            .channel
            .send(ServerMessage::PlayerMoves(SPlayerMoves { tick, moves }));
    }
}

// Start the firework show everywhere: broadcast a seed and forget — every
// client derives the same choreography from it.
pub fn broadcast_firework_show(players: &PlayerMap) {
    broadcast_to_all(
        players,
        ServerMessage::Firework(SFirework {
            seed: rand::random::<u64>(),
        }),
    );
}

pub fn broadcast_player_relocation(
    players: &PlayerMap,
    id: PlayerId,
    tick: u32,
    movement: PlayerMovementState,
    health: Health,
    portal_access: PortalAccess,
) {
    let Some(info) = players.get(&id) else { return };
    broadcast_to_all(
        players,
        ServerMessage::PlayerRelocated(SPlayerRelocated {
            id,
            tick,
            player: info.snapshot_player(movement, health, portal_access),
        }),
    );
}

// ============================================================================
// Data Collection Functions
// ============================================================================

// An active, alive player with the state both broadcasts read: the retained
// report and the entity's health.
struct ActivePlayer<'a> {
    id: PlayerId,
    info: &'a PlayerInfo,
    health: Health,
}

// The one player pass behind `SSnapshot.players` and `SPlayerMoves`.
fn active_players<'a>(
    players: &'a PlayerMap,
    player_data: &'a PlayerStateQuery,
) -> impl Iterator<Item = ActivePlayer<'a>> {
    players.iter().filter_map(|(player_id, info)| {
        if !info.connection.logged_in {
            return None;
        }
        // Death must surface as snapshot absence. A killed player's entity
        // despawn is deferred, so on a same-tick snapshot the corpse would
        // otherwise still resolve and ship here — after `SPlayerDeath`
        // already went out.
        let entity = info.entity()?;
        let (_, _, health) = player_data.get(entity).ok()?;
        Some(ActivePlayer {
            id: *player_id,
            info,
            health: *health,
        })
    })
}

// Collect all active, alive players for network updates.
#[must_use]
pub fn snapshot_active_players(
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    portal_assignments: &PortalAssignments,
) -> Vec<(PlayerId, Player)> {
    active_players(players, player_data)
        .map(|player| {
            (
                player.id,
                player.info.snapshot_player(
                    player.info.life.movement,
                    player.health,
                    portal_assignments.get(&player.id),
                ),
            )
        })
        .collect()
}

// Every active, alive player's retained report, for `SPlayerMoves`.
#[must_use]
pub fn collect_player_moves(players: &PlayerMap, player_data: &PlayerStateQuery) -> Vec<PlayerMove> {
    active_players(players, player_data)
        .map(|player| PlayerMove {
            id: player.id,
            generation: player.info.session.generation,
            movement: player.info.life.movement,
            seq: player.info.session.last_move_seq,
            portal_crossing: player.info.life.portal_crossing,
        })
        .collect()
}

// Collect all server-controlled actors for network updates.
#[must_use]
pub fn snapshot_actors(
    actors: &ActorMap,
    actor_data: &ActorStateQuery,
    motions: &ActorMotionQuery,
    carriers: &Carriers,
) -> Vec<(ActorId, Actor)> {
    active_actors(actors, actor_data, motions, carriers)
        .map(|(id, info, movement, health)| {
            (
                id,
                Actor {
                    kind: info.spawn_kind.clone(),
                    beam: info.beam.snapshot(),
                    movement,
                    health,
                },
            )
        })
        .collect()
}

pub fn collect_actor_moves(
    actors: &ActorMap,
    actor_data: &ActorStateQuery,
    motions: &ActorMotionQuery,
    carriers: &Carriers,
) -> Vec<ActorMove> {
    active_actors(actors, actor_data, motions, carriers)
        .map(|(id, _, movement, _)| ActorMove { id, movement })
        .collect()
}

fn active_actors<'a>(
    actors: &'a ActorMap,
    actor_data: &'a ActorStateQuery,
    motions: &'a ActorMotionQuery,
    carriers: &'a Carriers,
) -> impl Iterator<Item = (ActorId, &'a ActorInfo, ActorMovementState, Health)> {
    actors.iter().filter_map(|(actor_id, info)| {
        let (pos, move_intent, face_yaw, health) = actor_data.get(info.entity).ok()?;
        let (vertical, support) = motions.get(info.entity).ok()?;
        Some((
            *actor_id,
            info,
            ActorMovementState {
                pos: if info.flight.is_some() {
                    *pos
                } else {
                    carriers.pose(info.carrier).inverse_transform_position(pos)
                },
                carrier: if info.flight.is_some() {
                    CarrierId::WORLD
                } else {
                    info.carrier
                },
                move_intent: *move_intent,
                vertical_velocity: vertical.0,
                face_yaw: face_yaw.0,
                support: *support,
            },
            *health,
        ))
    })
}

// Collect reserved spawns still in their beam-in warning window. No entity
// query — pending spawns have no entity yet; everything lives in the resource.
#[must_use]
pub fn snapshot_spawning_actors(pending: &PendingActorSpawns) -> Vec<(ActorId, SpawningActor)> {
    pending
        .0
        .iter()
        .map(|spawn| {
            (
                spawn.actor_id,
                SpawningActor {
                    kind: spawn.kind.clone(),
                    carrier: spawn.carrier,
                    pos: spawn.pos,
                    face_yaw: spawn.face_yaw,
                    reserved_tick: spawn.reserved_tick,
                    due_tick: spawn.due_tick,
                },
            )
        })
        .collect()
}

// Collect in-flight missiles for the snapshot.
#[must_use]
pub fn snapshot_missiles(missiles: &MissileMap) -> Vec<(MissileId, Missile)> {
    missiles.iter().map(|(id, missile)| (*id, *missile)).collect()
}

// Build the authoritative item list that gets replicated to clients.
#[must_use]
pub fn collect_items(items: &ItemMap, item_positions: &Query<&Position, With<ItemMarker>>) -> Vec<(ItemId, Item)> {
    items
        .iter()
        // Placed items counting down their respawn exist server-side but are
        // invisible to clients until the timer elapses.
        .filter(|(_, info)| !info.is_hidden())
        .map(|(id, info)| {
            let pos_component = item_positions.get(info.entity).expect("Item entity missing Position");
            (
                *id,
                Item {
                    item_type: info.item_type,
                    carrier: info.carrier,
                    pos: *pos_component,
                },
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/broadcast.rs"]
mod tests;
