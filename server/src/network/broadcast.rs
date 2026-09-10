use bevy::prelude::*;

use crate::{
    actors::{ActorMap, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    missiles::{MissileMap, MissileVelocity},
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap, PlayerMotionQuery, PlayerStateQuery},
    portals::PortalAssignments,
};
use common::{
    physics::{CharacterVerticalVelocity, player_movement_state},
    protocol::*,
};

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

// An accepted crossing places the player for everyone; a rejection only
// tells the owner where to snap back to.
pub fn broadcast_portal_crossing(players: &PlayerMap, result: SPortalCrossed) {
    let message = ServerMessage::PortalCrossed(result);
    if result.accepted {
        broadcast_to_all(players, message);
    } else if let Some(owner) = players.get(&result.id).filter(|info| info.connection.logged_in) {
        let _ = owner.connection.channel.send(ServerToClient::Send(message));
    }
}

// ============================================================================
// Data Collection Functions
// ============================================================================

// An active, alive player with the per-tick state both broadcasts read.
struct ActivePlayer<'a> {
    id: PlayerId,
    info: &'a PlayerInfo,
    movement: PlayerMovementState,
    health: Health,
}

// The one player pass behind `SSnapshot.players` and `SPlayerMoves`.
fn active_players<'a>(
    players: &'a PlayerMap,
    player_data: &'a PlayerStateQuery,
    motions: &'a PlayerMotionQuery,
) -> impl Iterator<Item = ActivePlayer<'a>> {
    players.iter().filter_map(|(player_id, info)| {
        // Death must surface as snapshot absence. A killed player's entity
        // despawn is deferred, so on a same-tick snapshot the corpse would
        // otherwise still resolve and ship here — after `SPlayerDeath`
        // already went out.
        if !info.connection.logged_in {
            return None;
        }
        let entity = info.entity()?;
        let (pos, move_intent, face_yaw, health) = player_data.get(entity).ok()?;
        let (vertical_velocity, airborne_momentum, knockback) = motions.get(entity).ok()?;
        let movement = player_movement_state(
            *pos,
            *move_intent,
            face_yaw,
            vertical_velocity,
            airborne_momentum,
            knockback,
            info.life.fall_state.support(),
        );
        Some(ActivePlayer {
            id: *player_id,
            info,
            movement,
            health: *health,
        })
    })
}

// Collect all active, alive players for network updates.
#[must_use]
pub fn snapshot_active_players(
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    motions: &PlayerMotionQuery,
    portal_assignments: &PortalAssignments,
) -> Vec<(PlayerId, Player)> {
    active_players(players, player_data, motions)
        .map(|player| {
            (
                player.id,
                player
                    .info
                    .snapshot_player(player.movement, player.health, portal_assignments.get(&player.id)),
            )
        })
        .collect()
}

// Every active, alive player's movement state, for `SPlayerMoves`.
#[must_use]
pub fn collect_player_moves(
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    motions: &PlayerMotionQuery,
) -> Vec<PlayerMove> {
    active_players(players, player_data, motions)
        .map(|player| PlayerMove {
            id: player.id,
            movement: player.movement,
            move_seq: player.info.life.processed_move_seq,
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
                    anchor: info.anchor,
                    beam: info.beam.snapshot(),
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
    missiles: &MissileMap,
    missile_data: &Query<(&Position, &MissileVelocity), With<MissileMarker>>,
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
    use common::physics::{AirborneMomentum, KnockbackVelocity};
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
                AirborneMomentum::default(),
                KnockbackVelocity::default(),
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

        let mut state: SystemState<(PlayerStateQuery, PlayerMotionQuery)> = SystemState::new(&mut world);
        let (player_data, motions) = state.get(&world).expect("system params invalid for the test world");

        let snapshot = snapshot_active_players(
            &players,
            &player_data,
            &motions,
            &PortalAssignments::new(PortalMode::Both),
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

        let mut state: SystemState<(PlayerStateQuery, PlayerMotionQuery)> = SystemState::new(&mut world);
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
