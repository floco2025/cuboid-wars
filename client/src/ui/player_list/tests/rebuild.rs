use super::*;
use crate::players::PlayerInfo;
use common::protocol::{BarrierKindId, PlayerGeneration, PowerUpKind};

fn player(name: &str, score: i32) -> PlayerInfo {
    PlayerInfo {
        generation: PlayerGeneration(0),
        entity: Entity::PLACEHOLDER,
        score,
        name: name.to_owned(),
        power_ups: [false; PowerUpKind::COUNT],
        stunned: false,
        held_keys: Vec::new(),
        missiles: 0,
        last_movement_tick: 0,
        spawn_tick: 0,
    }
}

fn map(entries: Vec<(u32, PlayerInfo)>) -> PlayerMap {
    let mut map = PlayerMap::default();
    for (id, info) in entries {
        map.insert(PlayerId(id), info);
    }
    map
}

#[test]
fn content_hash_is_independent_of_insertion_order() {
    let forward = map(vec![(1, player("alice", 3)), (2, player("bob", 0))]);
    let reverse = map(vec![(2, player("bob", 0)), (1, player("alice", 3))]);

    assert_eq!(
        player_list_content_hash(&forward, Some(PlayerId(1))),
        player_list_content_hash(&reverse, Some(PlayerId(1)))
    );
}

#[test]
fn content_hash_changes_when_rendered_fields_change() {
    let base = map(vec![(1, player("alice", 3))]);
    let base_hash = player_list_content_hash(&base, Some(PlayerId(1)));

    let scored = map(vec![(1, player("alice", 4))]);
    assert_ne!(player_list_content_hash(&scored, Some(PlayerId(1))), base_hash);

    let renamed = map(vec![(1, player("alicia", 3))]);
    assert_ne!(player_list_content_hash(&renamed, Some(PlayerId(1))), base_hash);

    let mut with_key = player("alice", 3);
    with_key.held_keys.push(BarrierKindId(0));
    let keyed = map(vec![(1, with_key)]);
    assert_ne!(player_list_content_hash(&keyed, Some(PlayerId(1))), base_hash);

    let mut with_power_up = player("alice", 3);
    with_power_up.power_ups[PowerUpKind::Speed.index()] = true;
    let powered = map(vec![(1, with_power_up)]);
    assert_ne!(player_list_content_hash(&powered, Some(PlayerId(1))), base_hash);

    let mut with_missiles = player("alice", 3);
    with_missiles.missiles = 2;
    let armed = map(vec![(1, with_missiles)]);
    assert_ne!(player_list_content_hash(&armed, Some(PlayerId(1))), base_hash);

    let joined = map(vec![(1, player("alice", 3)), (2, player("bob", 0))]);
    assert_ne!(player_list_content_hash(&joined, Some(PlayerId(1))), base_hash);

    assert_ne!(player_list_content_hash(&base, Some(PlayerId(2))), base_hash);
}

#[test]
fn content_hash_ignores_fields_rendered_in_place() {
    let base = map(vec![(1, player("alice", 3))]);
    let base_hash = player_list_content_hash(&base, Some(PlayerId(1)));

    // Stun blinking updates the existing row without rebuilding the list.
    let mut in_place = player("alice", 3);
    in_place.stunned = true;
    let updated = map(vec![(1, in_place)]);

    assert_eq!(player_list_content_hash(&updated, Some(PlayerId(1))), base_hash);
}
