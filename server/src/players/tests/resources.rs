use bevy::prelude::*;
use bincode::config::standard;
use tokio::sync::mpsc::unbounded_channel;

use super::{PowerUpState, resources::*};
use crate::config::{
    ActorRespawnConfig, ActorRespawnScope, PlayerRespawnMode, PowerUpDurationSecs, PowerUpsConfig, RespawnConfig,
};
use common::{
    config::DeathTrigger,
    protocol::{
        BarrierKindId, Health, ItemType, PlayerId, PlayerMoveIntent, PlayerMovementState, PortalAccess, PortalPairId,
        Position, PowerUpKind, QuestId, SPlayerStatus,
    },
};

fn dummy_info() -> PlayerInfo {
    // Real channel + real Entity; we only exercise the held_keys path.
    let (tx, _rx) = unbounded_channel();
    PlayerInfo::new(Entity::PLACEHOLDER, tx)
}

fn test_power_ups_config() -> PowerUpsConfig {
    PowerUpsConfig {
        duration_secs: PowerUpDurationSecs {
            speed: 1.0,
            single_shot: 0.0,
            multi_shot: 1.0,
            low_gravity: 1.0,
            portal_gun: 0.0,
        },
    }
}

fn active_info() -> PlayerInfo {
    let mut info = dummy_info();
    info.connection.logged_in = true;
    info
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
                    let mut players = PlayerMap::new(RespawnConfig {
                        players: mode,
                        actors: ActorRespawnConfig {
                            on_player_death: trigger,
                            scope,
                        },
                    });
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
    let config = RespawnConfig {
        players: PlayerRespawnMode::Individual,
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::Solo,
            scope: ActorRespawnScope::All,
        },
    };
    let mut solo = PlayerMap::new(config);
    solo.insert(PlayerId(1), active_info());
    solo.begin_respawn(PlayerId(1), 2.0);
    solo.insert(PlayerId(2), active_info());
    assert_eq!(solo.tick_respawns(2.0).1, Some(ActorRespawnScope::All));

    let mut multiplayer = PlayerMap::new(config);
    multiplayer.insert(PlayerId(1), active_info());
    multiplayer.insert(PlayerId(2), active_info());
    multiplayer.begin_respawn(PlayerId(2), 2.0);
    multiplayer.begin_respawn(PlayerId(1), 2.0);
    multiplayer.disconnect(&PlayerId(2), 2.0);
    assert_eq!(multiplayer.tick_respawns(2.0), (vec![PlayerId(1)], None));
}

#[test]
fn all_actor_reset_waits_for_the_last_death_and_keeps_its_eligibility() {
    let mut players = PlayerMap::new(RespawnConfig {
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::All,
            scope: ActorRespawnScope::All,
        },
        ..default()
    });
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
    let mut players = PlayerMap::new(RespawnConfig {
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::All,
            scope: ActorRespawnScope::All,
        },
        ..default()
    });
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
                    let mut players = PlayerMap::new(RespawnConfig {
                        players: mode,
                        actors: ActorRespawnConfig {
                            on_player_death: trigger,
                            scope,
                        },
                    });
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
        let mut players = PlayerMap::new(RespawnConfig {
            players: mode,
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::Solo,
                scope: ActorRespawnScope::All,
            },
        });
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
    let mut players = PlayerMap::new(RespawnConfig {
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::Solo,
            scope: ActorRespawnScope::All,
        },
        ..default()
    });
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
    let mut players = PlayerMap::new(RespawnConfig {
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::Any,
            scope: ActorRespawnScope::All,
        },
        ..default()
    });
    players.insert(PlayerId(1), active_info());
    players.insert(PlayerId(0), dummy_info());
    players.disconnect(&PlayerId(0), 2.0);
    assert!(players.disconnect(&PlayerId(99), 2.0).is_none());
    assert!(players.take_resets().is_empty());
    assert_eq!(players.tick_respawns(2.0), (vec![], None));
}

#[test]
fn add_key_is_idempotent_and_keeps_sorted() {
    let mut info = dummy_info();
    assert!(info.add_key(BarrierKindId(2)));
    assert!(info.add_key(BarrierKindId(0)));
    assert!(info.add_key(BarrierKindId(1)));
    // Re-adding any already-held kind returns false (no state change).
    assert!(!info.add_key(BarrierKindId(0)));
    assert!(!info.add_key(BarrierKindId(1)));
    assert!(!info.add_key(BarrierKindId(2)));
    assert_eq!(
        info.life.held_keys,
        vec![BarrierKindId(0), BarrierKindId(1), BarrierKindId(2)]
    );
    assert!(info.has_key(BarrierKindId(1)));
    assert!(!info.has_key(BarrierKindId(3)));
}

#[test]
fn held_keys_round_trip_via_sp_player_status() {
    let mut info = dummy_info();
    info.add_key(BarrierKindId(1));
    info.add_key(BarrierKindId(3));
    let status = info.status(PlayerId(7));
    let encoded = bincode::encode_to_vec(&status, standard()).expect("encode");
    let (decoded, _): (SPlayerStatus, _) = bincode::decode_from_slice(&encoded, standard()).expect("decode");
    assert_eq!(decoded.held_keys, vec![BarrierKindId(1), BarrierKindId(3)]);
    assert_eq!(decoded.id, PlayerId(7));
}

#[test]
fn grant_power_up_sets_matching_status_flag() {
    let mut info = dummy_info();
    let durations = test_power_ups_config();

    info.grant_power_up(ItemType::SpeedPowerUp, &durations);
    info.grant_power_up(ItemType::MultiShotPowerUp, &durations);
    info.grant_power_up(ItemType::LowGravityPowerUp, &durations);

    let status = info.status(PlayerId(7));
    assert!(status.power_up(PowerUpKind::Speed));
    assert!(status.power_up(PowerUpKind::MultiShot));
    assert!(status.power_up(PowerUpKind::LowGravity));
}

#[test]
fn single_shot_can_expire_or_last_until_death() {
    let mut info = dummy_info();
    let mut config = test_power_ups_config();
    config.duration_secs.single_shot = 2.0;
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
fn missing_or_expired_gun_rejects_portal_fire() {
    let mut info = dummy_info();
    info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
    assert!(!info.try_start_portal_shot(1.0, 0.1));
    assert!(info.has(PowerUpKind::SingleShot));
    let mut config = test_power_ups_config();
    config.duration_secs.portal_gun = 2.0;
    info.grant_power_up(ItemType::PortalGunPowerUp, &config);
    info.tick_timers(1.0);
    assert!(info.try_start_portal_shot(2.0, 0.1));
    info.grant_power_up(ItemType::PortalGunPowerUp, &config);
    info.tick_timers(1.5);
    assert!(info.has(PowerUpKind::PortalGun));
    info.tick_timers(0.5);
    assert!(!info.try_start_portal_shot(3.0, 0.1));
    assert!(info.has(PowerUpKind::SingleShot));
}

#[test]
fn erasure_clears_power_ups_and_ammo_but_preserves_keys_and_progress() {
    let mut info = dummy_info();
    info.session.score = 42;
    info.session
        .quest_states
        .insert(QuestId("quest".into()), PlayerQuestState::Individual { progress: 3 });
    info.life.stun_timer = 2.0;
    info.add_key(BarrierKindId(1));
    info.add_missiles(2, 3);
    for kind in PowerUpKind::ALL {
        info.grant_power_up(kind.to_item_type(), &test_power_ups_config());
    }
    assert!(info.erase_equipment());
    assert!(!info.erase_equipment());
    assert!(PowerUpKind::ALL.into_iter().all(|kind| !info.has(kind)));
    assert_eq!(info.life.held_keys, [BarrierKindId(1)]);
    assert_eq!(info.life.missiles, 0);
    assert_eq!(info.life.stun_timer, 2.0);
    assert_eq!(info.session.score, 42);
    assert_eq!(
        info.session.quest_states[&QuestId("quest".into())].own_progress(),
        Some(3)
    );
}

#[test]
fn add_missiles_caps_at_max_and_reports_the_new_count() {
    let mut info = dummy_info();
    assert_eq!(info.add_missiles(2, 3), 2);
    assert_eq!(info.add_missiles(5, 3), 3, "adds clamp to the cap");
    assert_eq!(info.add_missiles(0, 3), 3, "zero add is a no-op");
    assert_eq!(info.life.missiles, 3);
}

#[test]
fn try_start_missile_requires_ammo() {
    let mut info = dummy_info();
    assert!(!info.try_start_missile(), "no ammo");

    info.add_missiles(2, 3);
    assert!(info.try_start_missile());
    assert_eq!(info.life.missiles, 1);
    assert!(info.try_start_missile());
    assert_eq!(info.life.missiles, 0);
    assert!(!info.try_start_missile(), "magazine empty");
}

#[test]
fn begin_respawn_zeroes_missiles() {
    let mut info = dummy_info();
    info.add_missiles(3, 3);

    info.begin_respawn(2.0);

    assert_eq!(info.life.missiles, 0);
    assert_eq!(info.entity(), None);
    assert_eq!(info.respawn_remaining_secs(), Some(2.0));

    let entity = Entity::from_bits(42);
    info.finish_respawn(entity);
    assert_eq!(info.entity(), Some(entity));
    assert_eq!(info.respawn_remaining_secs(), None);
}

#[test]
fn finish_respawn_preserves_life_state_changed_while_dead() {
    let mut info = dummy_info();
    info.begin_respawn(2.0);
    info.add_missiles(1, 3);

    info.finish_respawn(Entity::from_bits(42));

    assert_eq!(info.life.missiles, 1);
}

#[test]
fn snapshot_player_uses_same_status_fields_as_status_message() {
    let mut info = dummy_info();
    info.connection.name = "Alice".to_owned();
    info.session.score = 5;
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.0);
    info.life.power_ups[PowerUpKind::LowGravity.index()] = PowerUpState::Timed(2.0);
    info.life.stun_timer = 0.5;
    info.add_key(BarrierKindId(1));
    info.add_key(BarrierKindId(3));
    info.add_missiles(2, 3);
    let id = PlayerId(7);
    let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
    let move_intent = PlayerMoveIntent::Running { direction: 0.25 };
    let face_yaw = 1.5;
    let health = Health(42.0);
    let vertical_velocity = -3.0;
    let portal_access = PortalAccess::Both { pair: PortalPairId(1) };

    let status = info.status(id);
    let player = info.snapshot_player(
        PlayerMovementState::new(pos, move_intent, vertical_velocity, face_yaw),
        health,
        portal_access,
    );

    assert_eq!(player.name, info.connection.name);
    assert_eq!(player.score, info.session.score);
    assert_eq!(player.movement.pos, pos);
    assert_eq!(player.movement.move_intent, move_intent);
    assert_eq!(player.movement.vertical_velocity, vertical_velocity);
    assert_eq!(player.movement.face_yaw, face_yaw);
    assert_eq!(player.health, health);
    assert_eq!(player.power_ups, status.power_ups);
    assert_eq!(player.stunned, status.stunned);
    assert_eq!(player.held_keys, status.held_keys);
    assert_eq!(player.missiles, 2);
    assert_eq!(player.portal_access, portal_access);
}

#[test]
fn begin_respawn_preserves_session_state() {
    let quest_id = QuestId("collect_gold".to_owned());
    let mut info = dummy_info();
    info.session
        .quest_states
        .insert(quest_id.clone(), PlayerQuestState::Individual { progress: 7 });
    info.session.score = 42;
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(5.0);

    info.begin_respawn(2.0);

    assert_eq!(
        info.life.power_ups[PowerUpKind::Speed.index()],
        PowerUpState::Inactive,
        "power-up timers reset on death"
    );
    assert_eq!(
        info.session.quest_states[&quest_id].own_progress(),
        Some(7),
        "quest progress survives death"
    );
    assert_eq!(info.session.score, 42, "score survives death");
}

#[test]
fn a_blocked_respawns_logout_owes_the_full_actor_reset_delay() {
    let mut players = PlayerMap::new(RespawnConfig {
        players: PlayerRespawnMode::Individual,
        actors: ActorRespawnConfig {
            on_player_death: DeathTrigger::Any,
            scope: ActorRespawnScope::Dead,
        },
    });
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
