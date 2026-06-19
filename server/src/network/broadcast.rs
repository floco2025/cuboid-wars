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
            // Death must surface as snapshot absence. A killed player's
            // entity despawn is deferred, so on a same-tick snapshot the
            // corpse would otherwise still resolve and ship here — after
            // `SPlayerDeath` already went out.
            if !info.logged_in || info.is_dead() {
                return None;
            }
            let (pos, move_intent, face_dir, health) = player_data.get(info.entity).ok()?;
            let vertical_velocity = motions.get(info.entity).map_or(0.0, |m| m.0);
            Some((
                *player_id,
                info.snapshot_player(*pos, *move_intent, face_dir.0, *health, vertical_velocity),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::PlayerInfo;
    use bevy::ecs::system::SystemState;
    use tokio::sync::mpsc::unbounded_channel;

    fn spawn_player_entity(world: &mut World) -> Entity {
        world
            .spawn((
                Position::default(),
                PlayerMoveIntent::Idle,
                FaceDirection(0.0),
                Health(100.0),
                PlayerMarker,
                CharacterVerticalVelocity(0.0),
            ))
            .id()
    }

    fn logged_in_player(entity: Entity) -> PlayerInfo {
        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, tx);
        info.logged_in = true;
        info
    }

    #[test]
    fn snapshot_excludes_dead_players() {
        let mut world = World::new();
        let alive_entity = spawn_player_entity(&mut world);
        let dead_entity = spawn_player_entity(&mut world);

        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), logged_in_player(alive_entity));
        // A killed player's entity despawn is deferred, so the corpse is
        // still queryable on the snapshot tick — only `death_timer` marks it.
        let mut dead = logged_in_player(dead_entity);
        dead.death_timer = Some(2.0);
        players.insert(PlayerId(2), dead);

        let mut state: SystemState<(
            Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
            Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
        )> = SystemState::new(&mut world);
        let (player_data, motions) = state.get(&world).expect("query params are valid");

        let snapshot = snapshot_logged_in_players(&players, &player_data, &motions);

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].0, PlayerId(1));
    }
}
