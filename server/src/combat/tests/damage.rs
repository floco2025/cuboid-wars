use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{PendingExplosions, damage::*};
use crate::{
    actors::{ActorInfo, ActorMap},
    config::{
        ActorSettingsConfig, ActorsConfig, BlastConfig, CombatConfig, CyclesConfig, DamageConfig, FallDamageConfig,
        FeedConfig, HealthConfig, LightingCycleConfig, LightingMode, MapServerConfig, MissilesServerConfig,
        PlacedItemRespawnSecs, PlacedItemsConfig, PlayerHealthConfig, PowerUpDurationSecs, PowerUpsConfig,
        ScoringConfig, ServerGameplayConfig, WeaponsConfig, WeatherCycleConfig, WeatherMode,
    },
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap, PowerUpState},
};
use common::protocol::{
    ActorId, CarrierId, Health, PlayerId, PortalMode, Position, PowerUpKind, SPlayerDeath, ServerMessage,
};

fn logged_in_player(players: &mut PlayerMap, id: PlayerId, name: &str) -> UnboundedReceiver<ServerToClient> {
    let (tx, rx) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = true;
    info.connection.name = name.to_owned();
    players.insert(id, info);
    rx
}

fn next_player_death(receiver: &mut UnboundedReceiver<ServerToClient>) -> SPlayerDeath {
    loop {
        match receiver.try_recv().expect("expected a PlayerDeath broadcast") {
            ServerToClient::Send(ServerMessage::PlayerDeath(msg)) => return msg,
            _ => continue,
        }
    }
}

fn feed_lines(receiver: &mut UnboundedReceiver<ServerToClient>) -> Vec<String> {
    let mut lines = Vec::new();
    while let Ok(envelope) = receiver.try_recv() {
        if let ServerToClient::Send(ServerMessage::Feed(feed)) = envelope {
            lines.push(feed.spans.into_iter().map(|span| span.text).collect());
        }
    }
    lines
}

fn kill_with(players: &mut PlayerMap, victim: PlayerId, source: DeathSource) {
    let mut app = App::new();
    let world = app.world_mut();
    let entity = world.spawn_empty().id();
    let mut commands_queue = bevy::ecs::world::CommandQueue::default();
    let mut pending_explosions = PendingExplosions::default();
    {
        let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
        kill_player(
            &mut commands,
            players,
            victim,
            entity,
            Position::default(),
            2.0,
            source,
            &server_gameplay_config(),
            &mut pending_explosions,
        );
    }
    commands_queue.apply(world);
}

fn server_gameplay_config() -> ServerGameplayConfig {
    let default = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let movement = default
        .maps
        .get("hotel")
        .expect("hotel map settings missing")
        .settings
        .movement
        .clone();
    ServerGameplayConfig {
        network: Default::default(),
        default_map: "hotel".to_owned(),
        maps: HashMap::from([(
            "hotel".to_owned(),
            MapServerConfig {
                settings: common::protocol::MapSettings {
                    skybox: "cloudy_day".to_owned(),
                    textures: Default::default(),

                    geometry: crate::test_geometry::sizes(),
                    movement,
                    portals: PortalMode::Both,
                    switches: Vec::new(),
                    barrier_kinds: Vec::new(),
                    bridge_kinds: Vec::new(),
                },
                random_items: None,
                fireworks: None,
                player_fall: FallDamageConfig {
                    safe_distance: 4.0,
                    lethal_distance: 12.0,
                },
                respawn: Default::default(),
                power_ups: PowerUpsConfig {
                    duration_secs: PowerUpDurationSecs {
                        speed: 1.0,
                        single_shot: 0.0,
                        multi_shot: 1.0,
                        low_gravity: 1.0,
                        portal_gun: 0.0,
                    },
                },
                placed_items: PlacedItemsConfig {
                    respawn_secs: PlacedItemRespawnSecs {
                        speed: 60.0,
                        single_shot: 0.0,
                        multi_shot: 60.0,
                        low_gravity: 60.0,
                        portal_gun: 0.0,
                        health_potion: 60.0,
                        gold: 60.0,
                        key: 30.0,
                        missile_pack: 30.0,
                    },
                },
                weather: WeatherMode::Clear,
                lighting: LightingMode::Bright,
                quests: Vec::new(),
            },
        )]),
        player: default.player,
        actors: ActorsConfig {
            settings: ActorSettingsConfig {
                spawn_warning_secs: 0.0,
                threat_memory_secs: 0.0,
            },
            kinds: HashMap::new(),
        },
        weapons: WeaponsConfig {
            projectiles: default.weapons.projectiles,
            missiles: MissilesServerConfig {
                gameplay: default.weapons.missiles.gameplay,
                missiles_per_pack: 1,
            },
            portals: default.weapons.portals,
        },
        scoring: ScoringConfig {
            player_kill: 1,
            player_death: -1,
            gold: 1,
            actor_hit: HashMap::from([("zapper".to_owned(), 1)]),
            actor_kill: HashMap::from([("zapper".to_owned(), 10)]),
        },
        combat: CombatConfig {
            health: HealthConfig {
                player: PlayerHealthConfig {
                    max: 100.0,
                    regen_rate: 0.0,
                    potion_heal: 0.25,
                },
                actors: HashMap::new(),
            },
            damage: DamageConfig {
                projectile: 25.0,
                missile_blast: BlastConfig {
                    radius: 6.0,
                    max_damage: 105.0,
                },
                player_blast: BlastConfig {
                    radius: 10.0,
                    max_damage: 50.0,
                },
                actors: HashMap::new(),
            },
        },
        cycles: CyclesConfig {
            weather: WeatherCycleConfig {
                min_clear_secs: 10.0,
                max_clear_secs: 20.0,
                min_rain_secs: 5.0,
                max_rain_secs: 8.0,
                ramp_in_secs: 2.0,
                fade_out_secs: 4.0,
            },
            lighting: LightingCycleConfig {
                bright_secs: Some(20.0),
                dim_secs: Some(6.0),
                dark_secs: Some(10.0),
                bright_dim_secs: Some(4.0),
                dim_dark_secs: Some(2.0),
                bright_dark_secs: None,
            },
        },
        feed: FeedConfig::all(true, &[]),
    }
}

fn make_player_info() -> PlayerInfo {
    let (tx, _rx) = unbounded_channel();
    PlayerInfo::new(Entity::PLACEHOLDER, tx)
}

fn make_player_map_with(shooter: PlayerId, target: PlayerId) -> PlayerMap {
    let mut map = PlayerMap::default();
    map.insert(shooter, make_player_info());
    map.insert(target, make_player_info());
    map
}

#[test]
fn nonlethal_hit_returns_survived_and_leaves_score_alone() {
    let players = make_player_map_with(PlayerId(1), PlayerId(2));
    let mut health = Health(100.0);

    let was_lethal = apply_player_projectile_hit(&players, PlayerId(2), &mut health, &server_gameplay_config(), false);

    assert!(!was_lethal);
    assert_eq!(health.0, 75.0);
    assert_eq!(players.get(&PlayerId(1)).expect("shooter").session.score, 0);
    assert_eq!(players.get(&PlayerId(2)).expect("target").session.score, 0);
}

#[test]
fn repeated_hits_then_a_kill_charge_the_death_once() {
    let mut players = PlayerMap::default();
    logged_in_player(&mut players, PlayerId(1), "Bob");
    logged_in_player(&mut players, PlayerId(2), "Alex");
    let config = server_gameplay_config();
    let mut health = Health(100.0);
    let mut lethal = false;
    for _ in 0..4 {
        lethal = apply_player_projectile_hit(&players, PlayerId(2), &mut health, &config, false);
    }
    assert!(lethal);
    assert_eq!(players.get(&PlayerId(1)).expect("shooter").session.score, 0);
    assert_eq!(players.get(&PlayerId(2)).expect("target").session.score, 0);

    kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(1)));

    assert_eq!(
        players.get(&PlayerId(1)).expect("shooter").session.score,
        config.scoring.player_kill
    );
    assert_eq!(
        players.get(&PlayerId(2)).expect("target").session.score,
        config.scoring.player_death
    );
}

#[test]
fn lethal_hit_returns_true() {
    let players = make_player_map_with(PlayerId(1), PlayerId(2));
    let mut health = Health(10.0);

    let was_lethal = apply_player_projectile_hit(&players, PlayerId(2), &mut health, &server_gameplay_config(), false);

    assert!(was_lethal);
    assert_eq!(health.0, 0.0);
}

#[test]
fn dead_player_takes_no_further_damage() {
    let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
    players.get_mut(&PlayerId(2)).expect("target").begin_respawn(2.0);
    let mut health = Health(0.0);

    let was_lethal = apply_player_projectile_hit(&players, PlayerId(2), &mut health, &server_gameplay_config(), false);

    assert!(!was_lethal);
    // Score must not move on a no-op hit.
    assert_eq!(players.get(&PlayerId(1)).expect("shooter").session.score, 0);
    assert_eq!(players.get(&PlayerId(2)).expect("target").session.score, 0);
}

#[test]
fn beam_damage_lethal_tick_returns_true() {
    let players = make_player_map_with(PlayerId(1), PlayerId(2));
    let mut health = Health(5.0);

    let lethal = apply_player_beam_damage(&players, PlayerId(2), &mut health, 100.0, false);

    assert!(lethal);
    assert_eq!(health.0, 0.0);
}

#[test]
fn dead_player_takes_no_beam_damage() {
    let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
    players.get_mut(&PlayerId(2)).expect("target").begin_respawn(2.0);
    let mut health = Health(50.0);

    let lethal = apply_player_beam_damage(&players, PlayerId(2), &mut health, 100.0, false);

    assert!(!lethal);
    assert_eq!(health.0, 50.0);
}

#[test]
fn invincible_player_takes_no_beam_damage() {
    let players = make_player_map_with(PlayerId(1), PlayerId(2));
    let mut health = Health(50.0);

    let lethal = apply_player_beam_damage(&players, PlayerId(2), &mut health, 100.0, true);

    assert!(!lethal);
    assert_eq!(health.0, 50.0);
}

#[test]
fn dead_actor_takes_no_further_hits_or_score() {
    let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
    let mut health = Health(1.0);

    let first_hit_lethal = apply_actor_projectile_hit(&mut players, &PlayerId(1), "zapper", &mut health, &config);
    assert!(first_hit_lethal);
    let score_after_kill = players.get(&PlayerId(1)).expect("shooter").session.score;

    // The dying actor's entity stays queryable until removal runs later
    // in the tick; a same-tick second hit must not count as lethal again.
    let second_hit_lethal = apply_actor_projectile_hit(&mut players, &PlayerId(1), "zapper", &mut health, &config);
    assert!(!second_hit_lethal);
    assert_eq!(
        players.get(&PlayerId(1)).expect("shooter").session.score,
        score_after_kill
    );
}

#[test]
fn kill_player_broadcasts_player_death() {
    use tokio::sync::mpsc::unbounded_channel;

    let mut app = App::new();
    let mut players = PlayerMap::default();

    // Receiver with a logged-in shooter so the broadcast can reach them.
    let (shooter_tx, mut shooter_rx) = unbounded_channel();
    let mut shooter = PlayerInfo::new(Entity::PLACEHOLDER, shooter_tx);
    shooter.connection.logged_in = true;
    players.insert(PlayerId(1), shooter);

    // The dying player; also logged_in so the broadcast targets them too.
    let mut target = make_player_info();
    target.connection.logged_in = true;
    let target_entity = target.entity().expect("new player has no entity");
    players.insert(PlayerId(2), target);

    let world = app.world_mut();
    let mut commands_queue = bevy::ecs::world::CommandQueue::default();
    let mut pending_explosions = PendingExplosions::default();
    {
        let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
        kill_player(
            &mut commands,
            &mut players,
            PlayerId(2),
            target_entity,
            Position::default(),
            2.0,
            DeathSource::Shot(PlayerId(1)),
            &server_gameplay_config(),
            &mut pending_explosions,
        );
    }
    commands_queue.apply(world);
    assert_eq!(pending_explosions.0.len(), 1, "death must queue an explosion");

    let envelope = shooter_rx.try_recv().expect("shooter should have received PlayerDeath");
    match envelope {
        crate::network::ServerToClient::Send(ServerMessage::PlayerDeath(death)) => {
            assert_eq!(death.id, PlayerId(2));
        }
        other => panic!("unexpected message: {other:?}"),
    }
}

#[test]
fn kill_player_announces_death_with_cause() {
    let mut players = PlayerMap::default();
    let mut shooter_rx = logged_in_player(&mut players, PlayerId(1), "Bob");
    logged_in_player(&mut players, PlayerId(2), "Alex");

    kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(1)));

    assert_eq!(next_player_death(&mut shooter_rx).killer, Some(PlayerId(1)));
    assert_eq!(feed_lines(&mut shooter_rx), ["Bob shot Alex"]);
}

#[test]
fn self_shot_yields_no_credit_but_self_cause() {
    let mut players = PlayerMap::default();
    let mut rx = logged_in_player(&mut players, PlayerId(2), "Alex");

    kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(2)));

    assert_eq!(next_player_death(&mut rx).killer, None);
    assert_eq!(feed_lines(&mut rx), ["Alex shot themselves"]);
    assert_eq!(
        players.get(&PlayerId(2)).expect("victim").session.score,
        server_gameplay_config().scoring.player_death
    );
}

#[test]
fn kill_credit_ignores_departed_shooter() {
    let mut players = PlayerMap::default();
    logged_in_player(&mut players, PlayerId(1), "Bob");
    logged_in_player(&mut players, PlayerId(2), "Alex");

    assert_eq!(
        kill_credit(&DeathSource::Shot(PlayerId(9)), PlayerId(2), &players),
        None
    );
    assert_eq!(
        kill_credit(&DeathSource::Missile(PlayerId(1)), PlayerId(2), &players),
        Some(PlayerId(1))
    );
    assert_eq!(
        kill_credit(&DeathSource::PlayerBlast(PlayerId(1)), PlayerId(2), &players),
        None
    );
    assert_eq!(kill_credit(&DeathSource::Fall, PlayerId(2), &players), None);
}

#[test]
fn kill_actor_announces_only_flagged_kinds() {
    let mut feed = FeedConfig::all(false, &["bruiser", "zapper"]);
    feed.actor_destroyed.insert("bruiser".to_owned(), true);
    let mut players = PlayerMap::default();
    let mut rx = logged_in_player(&mut players, PlayerId(1), "Bob");
    let mut app = App::new();
    let world = app.world_mut();
    let bruiser = world.spawn_empty().id();
    let zapper = world.spawn_empty().id();
    let uncredited = world.spawn_empty().id();
    let mut actors = ActorMap::default();
    actors.insert(
        ActorId(1),
        ActorInfo::new(bruiser, 0, "bruiser".to_owned(), CarrierId::WORLD),
    );
    actors.insert(
        ActorId(2),
        ActorInfo::new(zapper, 0, "zapper".to_owned(), CarrierId::WORLD),
    );
    actors.insert(
        ActorId(3),
        ActorInfo::new(uncredited, 0, "bruiser".to_owned(), CarrierId::WORLD),
    );
    let mut pending_explosions = PendingExplosions::default();
    let mut commands_queue = bevy::ecs::world::CommandQueue::default();
    {
        let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
        for (id, entity, killer) in [
            (ActorId(2), zapper, Some(PlayerId(1))),
            (ActorId(1), bruiser, Some(PlayerId(1))),
            (ActorId(3), uncredited, None),
        ] {
            kill_actor(
                &mut commands,
                &mut actors,
                &players,
                &mut pending_explosions,
                &feed,
                id,
                entity,
                Position::default(),
                killer,
            );
        }
    }
    commands_queue.apply(world);

    assert_eq!(feed_lines(&mut rx), ["Bob destroyed a bruiser"]);
}

#[test]
fn kill_player_clears_state_and_arms_timer() {
    let mut app = App::new();
    let mut players = PlayerMap::default();
    let info = make_player_info();
    let entity = info.entity().expect("new player has no entity");
    let mut info = info;
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.5);
    info.add_key(common::protocol::BarrierKindId(0));
    players.insert(PlayerId(7), info);

    let world = app.world_mut();
    let mut commands_queue = bevy::ecs::world::CommandQueue::default();
    let mut pending_explosions = PendingExplosions::default();
    {
        let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
        kill_player(
            &mut commands,
            &mut players,
            PlayerId(7),
            entity,
            Position::default(),
            2.0,
            DeathSource::Fall,
            &server_gameplay_config(),
            &mut pending_explosions,
        );
    }
    commands_queue.apply(world);

    let info = players.get(&PlayerId(7)).expect("player still tracked after death");
    assert_eq!(info.respawn_remaining_secs(), Some(2.0));
    assert_eq!(info.life.power_ups, [PowerUpState::Inactive; PowerUpKind::COUNT]);
    assert!(info.life.held_keys.is_empty());
    assert_eq!(info.entity(), None);
    assert!(info.is_dead());
}

#[test]
fn void_fall_queues_no_explosion() {
    let mut app = App::new();
    let mut players = PlayerMap::default();
    let info = make_player_info();
    let entity = info.entity().expect("new player has no entity");
    let mut info = info;
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.5);
    info.add_key(common::protocol::BarrierKindId(0));
    players.insert(PlayerId(7), info);

    let world = app.world_mut();
    let mut commands_queue = bevy::ecs::world::CommandQueue::default();
    let mut pending_explosions = PendingExplosions::default();
    {
        let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
        kill_player(
            &mut commands,
            &mut players,
            PlayerId(7),
            entity,
            Position::default(),
            2.0,
            DeathSource::Void,
            &server_gameplay_config(),
            &mut pending_explosions,
        );
    }
    commands_queue.apply(world);
    assert!(
        pending_explosions.0.is_empty(),
        "a void fall must not queue an explosion"
    );
}

#[test]
fn begin_respawn_zeros_powerups_keys_and_cooldown() {
    let mut info = make_player_info();
    info.life.power_ups = [PowerUpState::Timed(1.0); PowerUpKind::COUNT];
    info.life.stun_timer = 1.0;
    info.life.last_portal_shot_time = 99.0;
    info.add_key(common::protocol::BarrierKindId(0));

    info.begin_respawn(2.0);

    assert_eq!(info.life.power_ups, [PowerUpState::Inactive; PowerUpKind::COUNT]);
    assert_eq!(info.life.stun_timer, 0.0);
    assert_eq!(info.life.last_portal_shot_time, f32::NEG_INFINITY);
    assert!(info.life.held_keys.is_empty());
}

#[test]
fn damage_does_not_go_below_zero() {
    let mut health = Health(20.0);
    apply_damage(&mut health, 30.0);
    assert_eq!(health, Health(0.0));
}
