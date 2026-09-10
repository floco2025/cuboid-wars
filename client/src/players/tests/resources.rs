use super::*;
use common::protocol::{Health, PlayerMoveIntent, PlayerMovementState, PortalAccess, Position};

fn snapshot_player() -> Player {
    Player {
        generation: PlayerGeneration(0),
        name: "Alice".to_owned(),
        movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::default(), 0.0, 0.0),
        health: Health(100.0),
        score: 7,
        power_ups: [false, true, false, true, false],
        stunned: true,
        held_keys: vec![BarrierKindId(1), BarrierKindId(3)],
        missiles: 2,
        portal_access: PortalAccess::None,
    }
}

#[test]
fn from_snapshot_copies_player_info_state() {
    let player = snapshot_player();

    let info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 42);

    assert_eq!(info.entity, Entity::PLACEHOLDER);
    assert_eq!(info.score, player.score);
    assert_eq!(info.name, player.name);
    assert_eq!(info.power_ups, player.power_ups);
    assert_eq!(info.stunned, player.stunned);
    assert_eq!(info.held_keys, player.held_keys);
    assert_eq!(info.missiles, player.missiles);
    assert_eq!(info.last_movement_tick, 42);
}

#[test]
fn death_blocks_delayed_snapshots_and_cues_but_allows_the_next_body() {
    for generation in [PlayerGeneration(0), PlayerGeneration(u32::MAX)] {
        let id = PlayerId(1);
        let mut player = snapshot_player();
        player.generation = generation;
        let mut players = PlayerMap::default();
        players.insert(id, PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 10));
        assert!(players.retire_body(id, generation));
        assert!(!players.accepts_generation(id, generation));
        assert!(players.accepts_death(id, generation));
        assert!(!players.accepts_body_cue(id, generation));
        players.remove(&id);
        assert!(!players.accepts_generation(id, generation));
        assert!(players.accepts_death(id, generation));
        player.generation = generation.next();
        assert!(players.accepts_generation(id, player.generation));
        players.insert(id, PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 12));
        assert!(players.accepts_body_cue(id, player.generation));
        assert!(!players.retire_body(id, generation));
        assert!(!players.accepts_body_cue(id, generation));
    }
}

#[test]
fn apply_status_updates_status_fields_only() {
    let player = snapshot_player();
    let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 0);
    let status = SPlayerStatus {
        id: PlayerId(12),
        generation: PlayerGeneration(0),
        collected: None,
        power_ups: [false, false, false, false, true],
        stunned: false,
        held_keys: vec![BarrierKindId(2)],
        missiles: 0,
    };

    info.apply_status(&status);

    assert_eq!(info.score, player.score);
    assert_eq!(info.name, player.name);
    assert_eq!(info.power_ups, status.power_ups);
    assert_eq!(info.stunned, status.stunned);
    assert_eq!(info.held_keys, status.held_keys);
    assert_eq!(info.missiles, status.missiles);
}
