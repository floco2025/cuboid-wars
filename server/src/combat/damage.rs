use bevy::prelude::*;

use super::PendingExplosions;
use crate::{
    actors::ActorMap,
    config::{FeedConfig, ServerGameplayConfig},
    network::{DeathCause, FeedAudience, FeedEvent, broadcast_to_all, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, QuestEvent, record_event},
};
use common::{
    health::apply_damage,
    protocol::{ActorId, Health, PlayerId, Position, SActorDeath, SPlayerDeath, ServerMessage},
};

// What killed a player, by id. `kill_player` derives both the kill credit
// (`SPlayerDeath.killer`) and the feed's `DeathCause` from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeathSource {
    Shot(PlayerId),
    Missile(PlayerId),
    Beam { kind: String },
    PlayerBlast(PlayerId),
    ActorBlast { kind: String },
    Fall,
    Admin,
}

// Credit goes to a shooter other than the victim who is still connected;
// beams, death blasts, falls, and admin kills credit nobody.
#[must_use]
pub fn kill_credit(source: &DeathSource, victim: PlayerId, players: &PlayerMap) -> Option<PlayerId> {
    match source {
        DeathSource::Shot(by) | DeathSource::Missile(by) => (*by != victim && players.get(by).is_some()).then_some(*by),
        DeathSource::Beam { .. }
        | DeathSource::PlayerBlast(_)
        | DeathSource::ActorBlast { .. }
        | DeathSource::Fall
        | DeathSource::Admin => None,
    }
}

fn death_cause(source: &DeathSource, victim: PlayerId, players: &PlayerMap) -> DeathCause {
    match source {
        DeathSource::Shot(by) if *by == victim => DeathCause::SelfShot,
        DeathSource::Shot(by) => DeathCause::Shot {
            by: players.display_name(by),
        },
        DeathSource::Missile(by) if *by == victim => DeathCause::SelfMissile,
        DeathSource::Missile(by) => DeathCause::Missile {
            by: players.display_name(by),
        },
        DeathSource::Beam { kind } => DeathCause::Beam { kind: kind.clone() },
        DeathSource::PlayerBlast(by) => DeathCause::PlayerBlast {
            by: players.display_name(by),
        },
        DeathSource::ActorBlast { kind } => DeathCause::ActorBlast { kind: kind.clone() },
        DeathSource::Fall => DeathCause::Fall,
        DeathSource::Admin => DeathCause::Admin,
    }
}

// Run the death sequence for one player: replace its life with the dead
// lifecycle, queue the death explosion, despawn the entity, broadcast
// `SPlayerDeath` so clients run death-side effects on the impact tick
// instead of waiting a snapshot, and announce the feed line. Called from
// every code path that takes a player to zero health (projectile hits,
// beams, explosions, falls, `/kill`).
#[expect(
    clippy::too_many_arguments,
    reason = "the one-stop death sequence threads all death state"
)]
pub fn kill_player(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    entity: Entity,
    pos: Position,
    respawn_secs: f32,
    source: DeathSource,
    feed: &FeedConfig,
    pending_explosions: &mut PendingExplosions,
) {
    let killer = kill_credit(&source, id, players);
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.is_dead() {
        return;
    }
    info.begin_respawn(respawn_secs);
    // Every death detonates — `explosions_system` drains the queue
    // this tick and applies the blast (void falls at CHARACTER_FALL_DEATH_Y
    // are too deep for the blast to reach the map).
    pending_explosions.push_player(id, pos);
    commands.entity(entity).despawn();
    // Snapshot the post-death scores so the cue carries the early-apply
    // values (HUD bumps on impact tick rather than next snapshot).
    let victim_score = players.get(&id).map_or(0, |info| info.session.score);
    let killer_score = killer.and_then(|kid| players.get(&kid)).map(|info| info.session.score);
    broadcast_to_all(
        players,
        ServerMessage::PlayerDeath(SPlayerDeath {
            id,
            pos,
            killer,
            victim_score,
            killer_score,
        }),
    );
    emit_feed(
        players,
        feed,
        FeedAudience::Everyone,
        FeedEvent::PlayerDied {
            name: players.display_name(&id),
            cause: death_cause(&source, id, players),
        },
    );
}

#[expect(clippy::too_many_arguments, reason = "the one-stop actor death sequence")]
pub fn kill_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    players: &PlayerMap,
    pending_explosions: &mut PendingExplosions,
    feed: &FeedConfig,
    id: ActorId,
    entity: Entity,
    pos: Position,
    killer: Option<PlayerId>,
) -> bool {
    let Some(info) = actors.remove(&id) else {
        return false;
    };
    if let Some(killer_id) = killer {
        emit_feed(
            players,
            feed,
            FeedAudience::Everyone,
            FeedEvent::ActorDestroyed {
                name: players.display_name(&killer_id),
                kind: info.spawn_kind.clone(),
            },
        );
    }
    let killer_score = killer
        .and_then(|killer_id| players.get(&killer_id))
        .map(|player| player.session.score);
    broadcast_to_all(
        players,
        ServerMessage::ActorDeath(SActorDeath {
            id,
            pos,
            killer,
            killer_score,
        }),
    );
    pending_explosions.push_actor(id, entity, info.spawn_kind, pos);
    commands.entity(entity).despawn();
    true
}

// Award the shooter's actor-kill credit: the per-kind score bonus plus any
// actor-kills quest progress. No-op when the shooter has disconnected.
// Shared by projectile lethal hits and missile blasts so the two paths
// can't drift.
pub fn award_actor_kill(
    players: &mut PlayerMap,
    quest_board: &mut QuestBoard,
    quest_catalog: &QuestCatalog,
    shooter_id: PlayerId,
    kind: &str,
    server_gameplay_config: &ServerGameplayConfig,
) {
    let Some(shooter) = players.get_mut(&shooter_id) else {
        return;
    };
    shooter.session.score += server_gameplay_config
        .scoring
        .actor_kill
        .get(kind)
        .copied()
        .expect("actor kind missing from scoring.actor_kill");
    record_event(
        players,
        quest_board,
        quest_catalog,
        &server_gameplay_config.feed,
        QuestEvent::ActorKilled {
            player: shooter_id,
            kind,
        },
    );
}

// Apply one projectile hit to a player. Returns `true` when this hit drops
// the target's health to zero (and the target wasn't already dead) — the
// caller is responsible for running `kill_player`.
pub fn apply_player_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    target_id: PlayerId,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
    invincible: bool,
) -> bool {
    // The projectile system shouldn't find a dead player (entity is gone),
    // but guard so a stray hit on a queued-for-despawn entity can't redeath.
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return false;
    }

    // Debug invincibility: cosmetic `SPlayerHit` still fires (camera shake
    // for the victim, hit sound for the shooter), but health and score are
    // untouched and the hit cannot be lethal.
    if invincible {
        return false;
    }

    apply_damage(target_health, server_gameplay_config.combat.damage.projectile);

    // Self-hits damage but don't score — the kill and death adjustments
    // would land on the same player and cancel out.
    if *shooter_id != target_id {
        let scoring = &server_gameplay_config.scoring;
        if let Some(shooter_info) = players.get_mut(shooter_id) {
            shooter_info.session.score += scoring.player_kill;
        }
        if let Some(target_info) = players.get_mut(&target_id) {
            target_info.session.score += scoring.player_death;
        }
    }

    target_health.0 <= 0.0
}

// Apply one tick of laser-beam contact to a player. `damage` is the per-tick
// amount (`beam_dps * dt`). Returns `true` when this tick drops
// the target to zero — the caller runs `kill_player(killer: None)`. No score
// adjustments: actor-inflicted, like falls and blasts.
pub fn apply_player_beam_damage(
    players: &PlayerMap,
    target_id: PlayerId,
    target_health: &mut Health,
    damage: f32,
    invincible: bool,
) -> bool {
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return false;
    }
    if invincible {
        return false;
    }
    apply_damage(target_health, damage);
    target_health.0 <= 0.0
}

// Apply one projectile hit to an actor. Returns `true` when this hit drops
// the target's health to zero — the caller awards the per-kind kill bonus
// (`scoring.actor_kill`).
pub fn apply_actor_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    kind: &str,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
) -> bool {
    // A dying actor's entity stays queryable until `actors_removal_system`
    // runs later in the tick; without this guard every further same-tick hit
    // would read the clamped 0 health as "lethal" and duplicate kill credit.
    if target_health.0 <= 0.0 {
        return false;
    }

    apply_damage(target_health, server_gameplay_config.combat.damage.projectile);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.session.score += server_gameplay_config
            .scoring
            .actor_hit
            .get(kind)
            .copied()
            .expect("actor kind missing from scoring.actor_hit");
    }

    target_health.0 <= 0.0
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::{
        ActorSettingsConfig, BlastConfig, CombatConfig, DamageConfig, FallDamageConfig, HealthConfig,
        LightingCycleConfig, LightingMode, MapServerConfig, MissilesServerConfig, PlacedItemRespawnSecs,
        PlacedItemsConfig, PlayerHealthConfig, PowerUpDurationSecs, PowerUpsConfig, ScoringConfig, WeatherCycleConfig,
        WeatherMode,
    };
    use crate::{actors::ActorInfo, network::ServerToClient, players::PlayerInfo};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn logged_in_player(players: &mut PlayerMap, id: PlayerId, name: &str) -> UnboundedReceiver<ServerToClient> {
        let (tx, rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.phase = crate::players::ConnectionPhase::Active;
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
                &FeedConfig::all(true, &[]),
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
        let barrier_kinds = default
            .maps
            .get("hotel")
            .expect("hotel map settings missing")
            .settings
            .barrier_kinds
            .clone();
        ServerGameplayConfig {
            player: default.player,
            projectiles: default.projectiles,
            portals: default.portals,
            maps: HashMap::from([(
                "hotel".to_owned(),
                MapServerConfig {
                    settings: common::protocol::MapSettings {
                        skybox: "cloudy_day".to_owned(),
                        movement,
                        weapons: common::protocol::MapWeaponSettings {
                            projectiles: true,
                            missiles: true,
                            portals: common::protocol::PortalMode::Both,
                        },
                        barrier_kinds,
                    },
                    random_items: None,
                    weather: WeatherMode::Clear,
                    lighting: LightingMode::Bright,
                    quests: Vec::new(),
                },
            )]),
            default_map: "hotel".to_owned(),
            weather_cycle: WeatherCycleConfig {
                min_clear_secs: 10.0,
                max_clear_secs: 20.0,
                min_rain_secs: 5.0,
                max_rain_secs: 8.0,
                ramp_in_secs: 2.0,
                fade_out_secs: 4.0,
            },
            lighting_cycle: LightingCycleConfig {
                bright_secs: Some(20.0),
                dim_secs: Some(6.0),
                dark_secs: Some(10.0),
                bright_dim_secs: Some(4.0),
                dim_dark_secs: Some(2.0),
                bright_dark_secs: None,
            },
            scoring: ScoringConfig {
                player_kill: 1,
                player_death: -1,
                cookie: 1,
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
                    player_fall: FallDamageConfig {
                        safe_distance: 4.0,
                        lethal_distance: 12.0,
                    },
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
            missiles: MissilesServerConfig {
                gameplay: default.missiles.gameplay,
                turn_radius: 1.7,
                lifetime_secs: 10.0,
                launch_spread_degrees: 50.0,
                weave_strength: 0.35,
                proximity_fuse_distance: 1.5,
                stall_secs: 2.0,
                missiles_per_pack: 1,
            },
            power_ups: PowerUpsConfig {
                duration_secs: PowerUpDurationSecs {
                    speed: 1.0,
                    multi_shot: 1.0,
                    low_gravity: 1.0,
                },
            },
            placed_items: PlacedItemsConfig {
                respawn_secs: PlacedItemRespawnSecs {
                    speed: 60.0,
                    multi_shot: 60.0,
                    low_gravity: 60.0,
                    health_potion: 60.0,
                    cookie: 60.0,
                    key: 30.0,
                    missile_pack: 30.0,
                },
            },
            actor_settings: ActorSettingsConfig {
                spawn_warning_secs: 0.0,
                threat_memory_secs: 0.0,
            },
            actors: HashMap::new(),
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
    fn nonlethal_hit_returns_survived_and_adjusts_score() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(100.0);

        let was_lethal = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
            false,
        );

        assert!(!was_lethal);
        assert_eq!(health.0, 75.0);
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").session.score, 1);
        assert_eq!(players.get(&PlayerId(2)).expect("target").session.score, -1);
    }

    #[test]
    fn lethal_hit_returns_true() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(10.0);

        let was_lethal = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
            false,
        );

        assert!(was_lethal);
        assert_eq!(health.0, 0.0);
    }

    #[test]
    fn self_hit_damages_but_does_not_score() {
        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), make_player_info());
        let mut health = Health(100.0);

        let was_lethal = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(1),
            &mut health,
            &server_gameplay_config(),
            false,
        );

        assert!(!was_lethal);
        assert_eq!(health.0, 75.0);
        assert_eq!(players.get(&PlayerId(1)).expect("player").session.score, 0);
    }

    #[test]
    fn dead_player_takes_no_further_damage() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        players.get_mut(&PlayerId(2)).expect("target").begin_respawn(2.0);
        let mut health = Health(0.0);

        let was_lethal = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
            false,
        );

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
        shooter.connection.phase = crate::players::ConnectionPhase::Active;
        players.insert(PlayerId(1), shooter);

        // The dying player; also logged_in so the broadcast targets them too.
        let mut target = make_player_info();
        target.connection.phase = crate::players::ConnectionPhase::Active;
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
                &FeedConfig::all(true, &[]),
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
        logged_in_player(&mut players, PlayerId(2), "Marc");

        kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(1)));

        assert_eq!(next_player_death(&mut shooter_rx).killer, Some(PlayerId(1)));
        assert_eq!(feed_lines(&mut shooter_rx), ["Bob shot Marc"]);
    }

    #[test]
    fn self_shot_yields_no_credit_but_self_cause() {
        let mut players = PlayerMap::default();
        let mut rx = logged_in_player(&mut players, PlayerId(2), "Marc");

        kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(2)));

        assert_eq!(next_player_death(&mut rx).killer, None);
        assert_eq!(feed_lines(&mut rx), ["Marc shot themselves"]);
    }

    #[test]
    fn kill_credit_ignores_departed_shooter() {
        let mut players = PlayerMap::default();
        logged_in_player(&mut players, PlayerId(1), "Bob");
        logged_in_player(&mut players, PlayerId(2), "Marc");

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
        let mut feed = FeedConfig::all(false, &["sentry", "zapper"]);
        feed.actor_destroyed.insert("sentry".to_owned(), true);
        let mut players = PlayerMap::default();
        let mut rx = logged_in_player(&mut players, PlayerId(1), "Bob");
        let mut app = App::new();
        let world = app.world_mut();
        let sentry = world.spawn_empty().id();
        let zapper = world.spawn_empty().id();
        let uncredited = world.spawn_empty().id();
        let mut actors = ActorMap::default();
        actors.insert(ActorId(1), ActorInfo::new(sentry, 0, "sentry".to_owned()));
        actors.insert(ActorId(2), ActorInfo::new(zapper, 0, "zapper".to_owned()));
        actors.insert(ActorId(3), ActorInfo::new(uncredited, 0, "sentry".to_owned()));
        let mut pending_explosions = PendingExplosions::default();
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();
        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            for (id, entity, killer) in [
                (ActorId(2), zapper, Some(PlayerId(1))),
                (ActorId(1), sentry, Some(PlayerId(1))),
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

        assert_eq!(feed_lines(&mut rx), ["Bob destroyed a sentry"]);
    }

    #[test]
    fn kill_player_clears_state_and_arms_timer() {
        let mut app = App::new();
        let mut players = PlayerMap::default();
        let info = make_player_info();
        let entity = info.entity().expect("new player has no entity");
        let mut info = info;
        info.life.power_up_timers[common::protocol::PowerUpKind::Speed.index()] = 1.5;
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
                &FeedConfig::all(true, &[]),
                &mut pending_explosions,
            );
        }
        commands_queue.apply(world);

        let info = players.get(&PlayerId(7)).expect("player still tracked after death");
        assert_eq!(info.respawn_remaining_secs(), Some(2.0));
        assert_eq!(info.life.power_up_timers, [0.0; common::protocol::PowerUpKind::COUNT]);
        assert!(info.life.held_keys.is_empty());
        assert_eq!(info.entity(), None);
        assert!(info.is_dead());
    }

    #[test]
    fn begin_respawn_zeros_powerups_keys_and_cooldown() {
        let mut info = make_player_info();
        info.life.power_up_timers = [1.0; common::protocol::PowerUpKind::COUNT];
        info.life.stun_timer = 1.0;
        info.life.last_shot_time = 99.0;
        info.add_key(common::protocol::BarrierKindId(0));

        info.begin_respawn(2.0);

        assert_eq!(info.life.power_up_timers, [0.0; common::protocol::PowerUpKind::COUNT]);
        assert_eq!(info.life.stun_timer, 0.0);
        assert_eq!(info.life.last_shot_time, f32::NEG_INFINITY);
        assert!(info.life.held_keys.is_empty());
    }
}
