use bevy::ecs::system::SystemState;
use crossbeam_channel::unbounded;

use super::*;

#[test]
fn snapshot_and_player_moves_exclude_dead_players() {
    let mut world = World::new();
    let mut players = PlayerMap::default();
    for (id, dead) in [(PlayerId(1), false), (PlayerId(2), true)] {
        let entity = world
            .spawn((Position::default(), FaceYaw(0.0), Health(100.0), PlayerMarker))
            .id();
        let (tx, _rx) = unbounded();
        let mut info = PlayerInfo::new(entity, tx);
        info.connection.logged_in = true;
        // A killed player's entity despawn is deferred, so the corpse is
        // still queryable on the snapshot tick; lifecycle state excludes it.
        if dead {
            info.begin_respawn(2.0);
        }
        players.insert(id, info);
    }

    let mut state: SystemState<PlayerStateQuery> = SystemState::new(&mut world);
    let player_data = state.get(&world).expect("system params invalid for the test world");

    let snapshot = snapshot_active_players(&players, &player_data, &PortalAssignments::new(PortalMode::Both));
    let moves = collect_player_moves(&players, &player_data);

    assert_eq!(snapshot.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [PlayerId(1)]);
    assert_eq!(moves.iter().map(|entry| entry.id).collect::<Vec<_>>(), [PlayerId(1)]);
}
