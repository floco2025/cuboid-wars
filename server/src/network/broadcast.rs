use bevy::prelude::*;

use crate::{
    actors::{ActorInfo, ActorMap, ActorMotionQuery, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    missiles::MissileMap,
    network::ServerToClient,
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
            .send(ServerToClient::Send(ServerMessage::PlayerMoves(SPlayerMoves {
                tick,
                moves,
            })));
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
                pos: carriers.pose(info.carrier).inverse_transform_position(pos),
                carrier: info.carrier,
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
mod tests {
    use super::*;
    use crate::players::PlayerInfo;
    use bevy::ecs::system::SystemState;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn spawn_player_entity(world: &mut World) -> Entity {
        world
            .spawn((Position::default(), FaceYaw(0.0), Health(100.0), PlayerMarker))
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

        let mut state: SystemState<PlayerStateQuery> = SystemState::new(&mut world);
        let player_data = state.get(&world).expect("system params invalid for the test world");

        let snapshot = snapshot_active_players(&players, &player_data, &PortalAssignments::new(PortalMode::Both));

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

        let mut state: SystemState<PlayerStateQuery> = SystemState::new(&mut world);
        let player_data = state.get(&world).expect("system params invalid for the test world");

        let moves = collect_player_moves(&players, &player_data);

        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].id, PlayerId(1));
    }

    #[test]
    fn player_moves_reach_every_other_client_without_the_recipients_own_entry() {
        let mut world = World::new();
        let mut players = PlayerMap::default();
        let mut receivers: Vec<(PlayerId, UnboundedReceiver<ServerToClient>)> = Vec::new();
        for id in [PlayerId(1), PlayerId(2), PlayerId(3)] {
            let entity = spawn_player_entity(&mut world);
            let (tx, rx) = unbounded_channel();
            let mut info = PlayerInfo::new(entity, tx);
            info.connection.logged_in = true;
            players.insert(id, info);
            receivers.push((id, rx));
        }
        let mut state: SystemState<PlayerStateQuery> = SystemState::new(&mut world);
        let player_data = state.get(&world).expect("system params invalid for the test world");
        let moves = collect_player_moves(&players, &player_data);
        assert_eq!(moves.len(), 3);

        broadcast_player_moves(&players, 9, &moves);

        for (id, receiver) in &mut receivers {
            let ServerToClient::Send(ServerMessage::PlayerMoves(message)) =
                receiver.try_recv().expect("movement batch missing")
            else {
                panic!("unexpected message");
            };
            assert_eq!(message.tick, 9);
            assert_eq!(message.moves.len(), 2);
            assert!(message.moves.iter().all(|entry| entry.id != *id));
            assert!(receiver.try_recv().is_err());
        }
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
                item_type: ItemType::Gold,
                placement: ItemPlacement::Placed { respawn_countdown: 0.0 },
                carrier: CarrierId::WORLD,
            },
        );
        items.insert(
            ItemId(2),
            ItemInfo {
                entity: hidden_entity,
                item_type: ItemType::Gold,
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
