use bevy::prelude::*;
use crossbeam_channel::unbounded;

use super::*;
use crate::{
    config::{ActorRespawnConfig, ActorRespawnScope, PlayerRespawnMode, PowerUpMode, PowerUpsConfig, RespawnConfig},
    players::PowerUpState,
};
use common::{
    config::DeathTrigger,
    protocol::{FieldId, ItemType, MapItems, PlayerId, PowerUpKind, QuestId},
};

fn dummy_info() -> PlayerInfo {
    let (tx, _rx) = unbounded();
    PlayerInfo::new(Entity::PLACEHOLDER, tx)
}

fn active_info() -> PlayerInfo {
    let mut info = dummy_info();
    info.connection.logged_in = true;
    info
}

fn player_map(players: PlayerRespawnMode, on_player_death: DeathTrigger, scope: ActorRespawnScope) -> PlayerMap {
    PlayerMap::new(
        RespawnConfig {
            players,
            actors: ActorRespawnConfig { on_player_death, scope },
        },
        Default::default(),
    )
}

fn test_power_ups_config() -> PowerUpsConfig {
    PowerUpsConfig {
        speed: PowerUpMode::Pickup {
            duration_secs: Some(1.0),
        },
        single_shot: PowerUpMode::Pickup { duration_secs: None },
        multi_shot: PowerUpMode::Pickup {
            duration_secs: Some(1.0),
        },
        low_gravity: PowerUpMode::Pickup {
            duration_secs: Some(1.0),
        },
        portal_gun: PowerUpMode::Pickup { duration_secs: None },
    }
}

#[test]
fn actor_respawn_policies_use_logged_in_counts_and_preserve_the_requested_scope() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        for (trigger, expected) in [
            (DeathTrigger::Never, [false, false]),
            (DeathTrigger::Solo, [true, false]),
            (DeathTrigger::Any, [true, true]),
            (DeathTrigger::All, [true, mode == PlayerRespawnMode::Group]),
        ] {
            for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
                for count in 1..=2 {
                    let mut players = player_map(mode, trigger, scope);
                    players.insert(PlayerId(0), dummy_info());
                    for id in 1..=count {
                        players.insert(PlayerId(id), active_info());
                    }
                    assert!(players.begin_respawn(PlayerId(1), 2.0));
                    assert!(!players.begin_respawn(PlayerId(1), 2.0));
                    assert_eq!(players.tick_respawns(1.0), (vec![], None));
                    let (ids, reset) = players.tick_respawns(1.0);
                    assert_eq!(ids, [PlayerId(1)]);
                    assert_eq!(reset, expected[count as usize - 1].then_some(scope));
                }
            }
        }
    }
}

#[test]
fn solo_actor_reset_eligibility_survives_membership_changes_and_counts_dead_players() {
    let solo_trigger = || {
        player_map(
            PlayerRespawnMode::Individual,
            DeathTrigger::Solo,
            ActorRespawnScope::All,
        )
    };
    let mut solo = solo_trigger();
    solo.insert(PlayerId(1), active_info());
    solo.begin_respawn(PlayerId(1), 2.0);
    solo.insert(PlayerId(2), active_info());
    assert_eq!(solo.tick_respawns(2.0).1, Some(ActorRespawnScope::All));

    let mut multiplayer = solo_trigger();
    multiplayer.insert(PlayerId(1), active_info());
    multiplayer.insert(PlayerId(2), active_info());
    multiplayer.begin_respawn(PlayerId(2), 2.0);
    multiplayer.begin_respawn(PlayerId(1), 2.0);
    multiplayer.disconnect(&PlayerId(2), 2.0);
    assert_eq!(multiplayer.tick_respawns(2.0), (vec![PlayerId(1)], None));
}

#[test]
fn all_actor_reset_waits_for_the_last_death_and_keeps_its_eligibility() {
    let mut players = player_map(PlayerRespawnMode::Individual, DeathTrigger::All, ActorRespawnScope::All);
    players.insert(PlayerId(1), active_info());
    players.insert(PlayerId(2), active_info());
    players.begin_respawn(PlayerId(1), 2.0);
    assert_eq!(players.tick_respawns(1.0), (vec![], None));
    players.begin_respawn(PlayerId(2), 2.0);
    assert_eq!(players.tick_respawns(1.0), (vec![PlayerId(1)], None));
    players
        .get_mut(&PlayerId(1))
        .expect("first player missing")
        .finish_respawn(Entity::PLACEHOLDER);
    players.insert(PlayerId(3), active_info());
    assert_eq!(
        players.tick_respawns(1.0),
        (vec![PlayerId(2)], Some(ActorRespawnScope::All))
    );
}

#[test]
fn disconnecting_the_last_survivor_arms_an_all_actor_reset() {
    let mut players = player_map(PlayerRespawnMode::Individual, DeathTrigger::All, ActorRespawnScope::All);
    players.insert(PlayerId(1), active_info());
    players.insert(PlayerId(2), active_info());
    players.begin_respawn(PlayerId(1), 2.0);
    players.disconnect(&PlayerId(2), 2.0);
    assert_eq!(
        players.tick_respawns(2.0),
        (vec![PlayerId(1)], Some(ActorRespawnScope::All))
    );
}

#[test]
fn logout_actor_policies_use_membership_before_departure_and_do_not_respawn_survivors() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        for (trigger, expected) in [
            (DeathTrigger::Never, [false, false]),
            (DeathTrigger::Solo, [true, false]),
            (DeathTrigger::Any, [true, true]),
            (DeathTrigger::All, [true, false]),
        ] {
            for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
                for count in 1..=2 {
                    let mut players = player_map(mode, trigger, scope);
                    players.insert(PlayerId(0), dummy_info());
                    for id in 1..=count {
                        players.insert(PlayerId(id), active_info());
                    }
                    players.disconnect(&PlayerId(1), 2.0);
                    assert!(players.disconnect(&PlayerId(1), 2.0).is_none());
                    assert!(!players.group_respawn_active());
                    assert!(players.values().all(|info| !info.is_dead()));
                    assert_eq!(players.tick_respawns(1.0), (vec![], None));
                    assert_eq!(
                        players.tick_respawns(1.0),
                        (vec![], expected[count as usize - 1].then_some(scope)),
                        "{mode:?}, {trigger:?}, {scope:?}, {count} players"
                    );
                    assert_eq!(players.tick_respawns(2.0), (vec![], None));
                }
            }
        }
    }
}

#[test]
fn logout_during_respawn_keeps_the_remaining_actor_countdown_when_the_server_empties() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        let mut players = player_map(mode, DeathTrigger::Solo, ActorRespawnScope::All);
        players.insert(PlayerId(1), active_info());
        players.begin_respawn(PlayerId(1), 2.0);
        assert_eq!(players.tick_respawns(1.0), (vec![], None));
        players.disconnect(&PlayerId(1), 2.0);
        assert!(!players.has_active_players());
        assert_eq!(players.tick_respawns(0.5), (vec![], None));
        assert_eq!(players.tick_respawns(0.5), (vec![], Some(ActorRespawnScope::All)));
        assert_eq!(players.tick_respawns(2.0), (vec![], None));
    }
}

#[test]
fn pending_actor_reset_survives_a_logout_that_no_longer_qualifies() {
    let mut players = player_map(
        PlayerRespawnMode::Individual,
        DeathTrigger::Solo,
        ActorRespawnScope::All,
    );
    players.insert(PlayerId(1), active_info());
    players.begin_respawn(PlayerId(1), 2.0);
    players.tick_respawns(1.0);
    players.insert(PlayerId(2), active_info());
    players.disconnect(&PlayerId(1), 2.0);
    assert_eq!(players.tick_respawns(1.0), (vec![], Some(ActorRespawnScope::All)));
    assert_eq!(players.tick_respawns(2.0), (vec![], None));
}

#[test]
fn unlogged_and_unknown_disconnects_do_not_trigger_world_resets() {
    let mut players = player_map(PlayerRespawnMode::Individual, DeathTrigger::Any, ActorRespawnScope::All);
    players.insert(PlayerId(1), active_info());
    players.insert(PlayerId(0), dummy_info());
    players.disconnect(&PlayerId(0), 2.0);
    assert!(players.disconnect(&PlayerId(99), 2.0).is_none());
    assert!(players.take_resets().is_empty());
    assert_eq!(players.tick_respawns(2.0), (vec![], None));
}

#[test]
fn a_blocked_respawns_logout_owes_the_full_actor_reset_delay() {
    let mut players = player_map(
        PlayerRespawnMode::Individual,
        DeathTrigger::Any,
        ActorRespawnScope::Dead,
    );
    players.insert(PlayerId(1), active_info());
    players.insert(PlayerId(2), active_info());
    assert!(players.begin_respawn(PlayerId(1), 2.0));
    // The death's own reset fires while the respawn stays blocked past its countdown.
    let (due, reset) = players.tick_respawns(3.0);
    assert_eq!(due, [PlayerId(1)]);
    assert!(reset.is_some());
    assert_eq!(
        players
            .get(&PlayerId(1))
            .expect("dead player missing")
            .respawn_remaining_secs(),
        Some(0.0),
        "the countdown holds at zero"
    );

    players.disconnect(&PlayerId(1), 2.0);
    assert!(
        players.tick_respawns(1.0).1.is_none(),
        "the logout waits the full delay"
    );
    assert!(players.tick_respawns(1.0).1.is_some());
}

#[test]
fn add_key_is_idempotent_and_keeps_sorted() {
    let mut info = dummy_info();
    assert!(info.add_key(FieldId(2)));
    assert!(info.add_key(FieldId(0)));
    assert!(info.add_key(FieldId(1)));
    // Re-adding any already-held kind returns false (no state change).
    assert!(!info.add_key(FieldId(0)));
    assert!(!info.add_key(FieldId(1)));
    assert!(!info.add_key(FieldId(2)));
    assert_eq!(info.life.held_keys, vec![FieldId(0), FieldId(1), FieldId(2)]);
    assert!(info.has_key(FieldId(1)));
    assert!(!info.has_key(FieldId(3)));
}

#[test]
fn single_shot_can_expire_or_last_until_death() {
    let mut info = dummy_info();
    let mut config = test_power_ups_config();
    config.single_shot = PowerUpMode::Pickup {
        duration_secs: Some(2.0),
    };
    info.grant_power_up(ItemType::SingleShotPowerUp, &config);
    assert!(info.has(PowerUpKind::SingleShot));
    info.tick_timers(2.0);
    assert!(!info.has(PowerUpKind::SingleShot));
    info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
    info.tick_timers(1000.0);
    assert!(info.has(PowerUpKind::SingleShot));
    info.begin_respawn(1.0);
    info.finish_respawn(Entity::PLACEHOLDER);
    assert!(!info.has(PowerUpKind::SingleShot));
}

#[test]
fn a_death_resets_the_life_not_the_session_and_the_respawn_keeps_later_changes() {
    let quest_id = QuestId("collect_gold".to_owned());
    let mut info = dummy_info();
    info.session
        .quest_states
        .insert(quest_id.clone(), PlayerQuestState::Individual { progress: 7 });
    info.session.score = 42;
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(5.0);
    info.add_missiles(3, 3);

    info.begin_respawn(2.0);

    assert_eq!(info.life.power_ups[PowerUpKind::Speed.index()], PowerUpState::Inactive);
    assert_eq!(info.life.missiles, 0);
    assert_eq!(info.entity(), None);
    assert_eq!(info.respawn_remaining_secs(), Some(2.0));
    assert_eq!(info.session.quest_states[&quest_id].own_progress(), Some(7));
    assert_eq!(info.session.score, 42);

    info.add_missiles(1, 3);
    let entity = Entity::from_bits(42);
    info.finish_respawn(entity);

    assert_eq!(info.entity(), Some(entity));
    assert_eq!(info.respawn_remaining_secs(), None);
    assert_eq!(info.life.missiles, 1);
}

#[test]
fn always_active_abilities_survive_equipment_and_life_resets() {
    for group in [false, true] {
        let mut config = test_power_ups_config();
        config.single_shot = PowerUpMode::Always {};
        let mut players = PlayerMap::new(RespawnConfig::default(), config.always_active());
        players.insert(PlayerId(1), active_info());
        let info = players.get_mut(&PlayerId(1)).expect("player missing");
        assert!(info.has(PowerUpKind::SingleShot));
        assert!(info.status(PlayerId(1)).power_up(PowerUpKind::SingleShot));
        assert!(!info.erase_equipment());
        info.grant_power_up(ItemType::SingleShotPowerUp, &config);
        info.grant_power_up(ItemType::SpeedPowerUp, &config);
        info.life.missiles = 2;
        assert!(info.erase_equipment());
        assert!(info.has(PowerUpKind::SingleShot));
        assert!(!info.has(PowerUpKind::Speed));
        assert_eq!(info.life.missiles, 0);
        if group {
            info.begin_group_respawn();
        } else {
            info.begin_respawn(1.0);
        }
        info.finish_respawn(Entity::PLACEHOLDER);
        info.tick_timers(1000.0);
        assert!(info.has(PowerUpKind::SingleShot));
        assert!(!info.erase_equipment());
        let pickups = MapItems(vec![]);
        assert!(players.players_can_be_armed(&pickups));
        assert!(!pickups.contains(ItemType::SingleShotPowerUp));
        assert!(!PlayerMap::default().players_can_be_armed(&pickups));
    }
}
