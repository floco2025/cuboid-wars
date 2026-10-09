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
        held_keys: vec![FieldId(1), FieldId(3)],
        missiles: 2,
        portal_access: PortalAccess::None,
        checkpoint: 3,
    }
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
fn checkpoint_cues_and_snapshots_share_wrap_aware_ordering() {
    for tick in [10_u32, u32::MAX - 1] {
        let mut player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, tick);
        info.apply_checkpoint(4, tick.wrapping_add(2));
        info.apply_snapshot(&player, tick.wrapping_add(1));
        assert_eq!(info.checkpoint, 4);
        player.checkpoint = 5;
        info.apply_snapshot(&player, tick.wrapping_add(3));
        info.apply_checkpoint(4, tick.wrapping_add(2));
        assert_eq!(info.checkpoint, 5);
        player.checkpoint = 0;
        info.apply_snapshot(&player, tick.wrapping_add(4));
        assert_eq!(info.checkpoint, 0);
    }
}
