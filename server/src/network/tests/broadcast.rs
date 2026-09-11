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
    let mut receivers: Vec<(PlayerId, UnboundedReceiver<ServerMessage>)> = Vec::new();
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
        let ServerMessage::PlayerMoves(message) = receiver.try_recv().expect("movement batch missing") else {
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
