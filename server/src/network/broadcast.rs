use bevy::prelude::*;

use crate::{
    actors::{ActorMap, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    network::ServerToClient,
    players::{PlayerMap, PlayerStateQuery},
    portals::PortalAssignments,
};
use common::{physics::CharacterVerticalVelocity, protocol::*};

// ============================================================================
// Broadcasting Helpers
// ============================================================================

// Broadcast `message` to every active player except `skip`.
pub fn broadcast_to_others(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in players.iter() {
        if *other_id != skip && other_info.connection.logged_in {
            let _ = other_info
                .connection
                .channel
                .send(ServerToClient::Send(message.clone()));
        }
    }
}

// Broadcast `message` to every active player.
pub fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.values() {
        if player_info.connection.logged_in {
            let _ = player_info
                .connection
                .channel
                .send(ServerToClient::Send(message.clone()));
        }
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

// ============================================================================
// Data Collection Functions
// ============================================================================

// Collect all active, alive players for network updates.
#[must_use]
pub fn snapshot_active_players(
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    portal_assignments: &PortalAssignments,
) -> Vec<(PlayerId, Player)> {
    players
        .iter()
        .filter_map(|(player_id, info)| {
            // Death must surface as snapshot absence. A killed player's
            // entity despawn is deferred, so on a same-tick snapshot the
            // corpse would otherwise still resolve and ship here — after
            // `SPlayerDeath` already went out.
            if !info.connection.logged_in {
                return None;
            }
            let entity = info.entity()?;
            let (pos, move_intent, face_yaw, health) = player_data.get(entity).ok()?;
            let vertical_velocity = motions.get(entity).map_or(0.0, |m| m.0);
            Some((
                *player_id,
                info.snapshot_player(
                    *pos,
                    *move_intent,
                    face_yaw.0,
                    *health,
                    vertical_velocity,
                    portal_assignments.get(player_id),
                ),
            ))
        })
        .collect()
}

// Every active, alive player's movement state, for `SPlayerMoves`.
#[must_use]
pub fn collect_player_moves(
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
) -> Vec<PlayerMove> {
    players
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.connection.logged_in {
                return None;
            }
            let entity = info.entity()?;
            let (pos, move_intent, face_yaw, _) = player_data.get(entity).ok()?;
            let vertical_velocity = motions.get(entity).map_or(0.0, |m| m.0);
            Some(PlayerMove {
                id: *player_id,
                movement: PlayerMovementState::new(*pos, *move_intent, vertical_velocity, face_yaw.0),
                move_seq: info.session.last_move_seq,
                hops: info.session.hops,
            })
        })
        .collect()
}

// Collect all server-controlled actors for network updates.
#[must_use]
pub fn snapshot_actors(
    actors: &ActorMap,
    actor_data: &ActorStateQuery,
    motions: &Query<&CharacterVerticalVelocity, With<ActorMarker>>,
) -> Vec<(ActorId, Actor)> {
    actors
        .iter()
        .filter_map(|(actor_id, info)| {
            let (pos, move_intent, face_yaw, health) = actor_data.get(info.entity).ok()?;
            let vertical_velocity = motions.get(info.entity).map_or(0.0, |m| m.0);
            Some((
                *actor_id,
                Actor {
                    kind: info.spawn_kind.clone(),
                    movement: ActorMovementState::new(*pos, *move_intent, vertical_velocity),
                    face_yaw: face_yaw.0,
                    health: *health,
                },
            ))
        })
        .collect()
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
pub fn snapshot_missiles(
    missiles: &crate::missiles::MissileMap,
    missile_data: &Query<(&Position, &crate::missiles::MissileVelocity), With<MissileMarker>>,
) -> Vec<(MissileId, Missile)> {
    missiles
        .iter()
        .filter_map(|(missile_id, info)| {
            let (pos, velocity) = missile_data.get(info.entity).ok()?;
            Some((
                *missile_id,
                Missile {
                    shooter: info.shooter,
                    movement: MissileMovementState::from_velocity(*pos, velocity.0),
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
mod tests {
    use super::*;
    use crate::players::PlayerInfo;
    use bevy::ecs::system::SystemState;
    use tokio::sync::mpsc::unbounded_channel;

    fn spawn_player_entity(world: &mut World) -> Entity {
        world
            .spawn((
                Position::default(),
                PlayerMoveIntent::Idle,
                FaceYaw(0.0),
                Health(100.0),
                PlayerMarker,
                CharacterVerticalVelocity(0.0),
            ))
            .id()
    }

    fn active_player(entity: Entity) -> PlayerInfo {
        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, tx);
        info.connection.logged_in = true;
        info
    }

    #[test]
    fn snapshot_excludes_dead_players() {
        let mut world = World::new();
        let alive_entity = spawn_player_entity(&mut world);
        let dead_entity = spawn_player_entity(&mut world);

        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), active_player(alive_entity));
        // A killed player's entity despawn is deferred, so the corpse is
        // still queryable on the snapshot tick; lifecycle state excludes it.
        let mut dead = active_player(dead_entity);
        dead.begin_respawn(2.0);
        players.insert(PlayerId(2), dead);

        let mut state: SystemState<(PlayerStateQuery, Query<&CharacterVerticalVelocity, With<PlayerMarker>>)> =
            SystemState::new(&mut world);
        let (player_data, motions) = state.get(&world).expect("system params invalid for the test world");

        let snapshot = snapshot_active_players(
            &players,
            &player_data,
            &motions,
            &PortalAssignments::new(PortalMode::None),
        );

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].0, PlayerId(1));
    }

    #[test]
    fn player_moves_exclude_dead_players() {
        let mut world = World::new();
        let alive_entity = spawn_player_entity(&mut world);
        let dead_entity = spawn_player_entity(&mut world);

        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), active_player(alive_entity));
        let mut dead = active_player(dead_entity);
        dead.begin_respawn(2.0);
        players.insert(PlayerId(2), dead);

        let mut state: SystemState<(PlayerStateQuery, Query<&CharacterVerticalVelocity, With<PlayerMarker>>)> =
            SystemState::new(&mut world);
        let (player_data, motions) = state.get(&world).expect("system params invalid for the test world");

        let moves = collect_player_moves(&players, &player_data, &motions);

        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].id, PlayerId(1));
    }

    #[test]
    fn collect_items_omits_hidden_placed_items() {
        use crate::items::{ItemInfo, ItemPlacement};

        let mut world = World::new();
        let visible_entity = world.spawn((ItemMarker, Position::default())).id();
        let hidden_entity = world.spawn((ItemMarker, Position::default())).id();

        let mut items = ItemMap::default();
        items.insert(
            ItemId(1),
            ItemInfo {
                entity: visible_entity,
                item_type: ItemType::Cookie,
                placement: ItemPlacement::Placed { respawn_countdown: 0.0 },
                carrier: CarrierId::WORLD,
            },
        );
        items.insert(
            ItemId(2),
            ItemInfo {
                entity: hidden_entity,
                item_type: ItemType::Cookie,
                placement: ItemPlacement::Placed { respawn_countdown: 5.0 },
                carrier: CarrierId::WORLD,
            },
        );

        let mut state: SystemState<Query<&Position, With<ItemMarker>>> = SystemState::new(&mut world);
        let item_positions = state.get(&world).expect("system params invalid for the test world");

        let snapshot = collect_items(&items, &item_positions);

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].0, ItemId(1));
    }
}
