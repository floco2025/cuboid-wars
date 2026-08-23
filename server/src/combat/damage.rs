use bevy::prelude::*;

use super::PendingExplosions;
use crate::{
    actors::ActorMap,
    config::ServerGameplayConfig,
    network::{ServerToClient, broadcast_to_all},
    players::{PlayerMap, QuestEvent, record_quest_event},
};
use common::{
    health::apply_damage,
    protocol::{ActorId, Health, PlayerId, Position, SActorDeath, SPlayerDeath, ServerMessage},
};

// Run the death sequence for one player: clear per-life state, arm the
// respawn timer, queue the death explosion, despawn the entity, and
// broadcast `SPlayerDeath` so clients run death-side effects on the impact
// tick instead of waiting a snapshot. Called from every code path that takes
// a player to zero health (projectile hits, explosions, falls).
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
    respawn_delay_secs: f32,
    killer: Option<PlayerId>,
    pending_explosions: &mut PendingExplosions,
) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.is_dead() {
        return;
    }
    info.clear_per_life_state();
    info.death_timer = Some(respawn_delay_secs);
    // Every death detonates — `explosions_system` drains the queue
    // this tick and applies the blast (void falls at CHARACTER_FALL_DEATH_Y
    // are too deep for the blast to reach the map).
    pending_explosions.push_player(id, pos);
    commands.entity(entity).despawn();
    // Snapshot the post-death scores so the cue carries the early-apply
    // values (HUD bumps on impact tick rather than next snapshot).
    let victim_score = players.get(&id).map_or(0, |info| info.score);
    let killer_score = killer.and_then(|kid| players.get(&kid)).map(|info| info.score);
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
}

pub fn kill_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    players: &PlayerMap,
    pending_explosions: &mut PendingExplosions,
    id: ActorId,
    entity: Entity,
    pos: Position,
    killer: Option<PlayerId>,
) -> bool {
    let Some(info) = actors.remove(&id) else {
        return false;
    };
    let killer_score = killer
        .and_then(|killer_id| players.get(&killer_id))
        .map(|player| player.score);
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
// actor-kills quest progress, unicast to the shooter. No-op when the shooter
// has disconnected. Shared by projectile lethal hits and missile blasts so
// the two paths can't drift.
pub fn award_actor_kill(
    players: &mut PlayerMap,
    shooter_id: PlayerId,
    kind: &str,
    server_gameplay_config: &ServerGameplayConfig,
) {
    let Some(shooter) = players.get_mut(&shooter_id) else {
        return;
    };
    shooter.score += server_gameplay_config
        .scoring
        .actor_kill
        .get(kind)
        .copied()
        .expect("actor kind missing from scoring.actor_kill");
    let quest_messages = record_quest_event(
        shooter,
        &server_gameplay_config.quests,
        &server_gameplay_config.scoring,
        QuestEvent::ActorKilled { kind },
    );
    for msg in quest_messages {
        let _ = shooter.channel.send(ServerToClient::Send(msg));
    }
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

    apply_damage(
        target_health,
        server_gameplay_config.projectile.damage * (1.0 - server_gameplay_config.player.armor),
    );

    // Self-hits damage but don't score — the kill and death adjustments
    // would land on the same player and cancel out.
    if *shooter_id != target_id {
        let scoring = &server_gameplay_config.scoring;
        if let Some(shooter_info) = players.get_mut(shooter_id) {
            shooter_info.score += scoring.player_kill;
        }
        if let Some(target_info) = players.get_mut(&target_id) {
            target_info.score += scoring.player_death;
        }
    }

    target_health.0 <= 0.0
}

// Apply one tick of laser-beam contact to a player. `damage` is the raw
// per-tick amount (`damage_per_second * dt`); the victim's armor applies
// here. Returns `true` when this tick drops the target to zero — the caller
// runs `kill_player(killer: None)`. No score adjustments: actor-inflicted,
// like falls and blasts.
pub fn apply_player_beam_damage(
    players: &PlayerMap,
    target_id: PlayerId,
    target_health: &mut Health,
    damage: f32,
    server_gameplay_config: &ServerGameplayConfig,
    invincible: bool,
) -> bool {
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return false;
    }
    if invincible {
        return false;
    }
    apply_damage(target_health, damage * (1.0 - server_gameplay_config.player.armor));
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

    apply_damage(target_health, server_gameplay_config.projectile.damage);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.score += server_gameplay_config
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
        ActorSettingsConfig, ExplosionDamageConfig, FallDamageConfig, LightingCycleConfig, LightingMode,
        MapServerConfig, MissilesServerConfig, PlacedItemRespawnSecs, PlacedItemsConfig, PlayerServerConfig,
        PowerUpsConfig, ProjectileConfig, ScoringConfig, WeatherCycleConfig, WeatherMode,
    };
    use crate::players::PlayerInfo;
    use tokio::sync::mpsc::unbounded_channel;

    fn server_gameplay_config() -> ServerGameplayConfig {
        ServerGameplayConfig {
            maps: HashMap::from([(
                "hotel".to_owned(),
                MapServerConfig {
                    settings: common::protocol::MapSettings {
                        skybox: "cloudy_day".to_owned(),
                        gravity: 25.0,
                        low_gravity: 5.0,
                    },
                    random_items: None,
                    weather: WeatherMode::Clear,
                    lighting: LightingMode::Bright,
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
                quest_completed: HashMap::new(),
            },
            player: PlayerServerConfig {
                armor: 0.0,
                fall_damage: FallDamageConfig {
                    safe_fall_distance: 4.0,
                    lethal_fall_distance: 12.0,
                },
                explosion: ExplosionDamageConfig {
                    radius: 10.0,
                    max_damage: 50.0,
                },
            },
            projectile: ProjectileConfig { damage: 25.0 },
            missiles: MissilesServerConfig {
                speed: 12.0,
                turn_rate: 7.0,
                lifetime_secs: 10.0,
                launch_spread_degrees: 50.0,
                weave_strength: 0.35,
                proximity_fuse_distance: 1.5,
                stall_secs: 2.0,
                max_damage: 105.0,
                missiles_per_pack: 1,
            },
            power_ups: PowerUpsConfig {
                speed_duration_secs: 1.0,
                multi_shot_duration_secs: 1.0,
                low_gravity_duration_secs: 1.0,
                health_potion_heal_fraction: 0.25,
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
            quests: Vec::new(),
            actor_settings: ActorSettingsConfig {
                spawn_warning_secs: 0.0,
                threat_memory_secs: 0.0,
            },
            actors: HashMap::new(),
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
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").score, 1);
        assert_eq!(players.get(&PlayerId(2)).expect("target").score, -1);
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
        assert_eq!(players.get(&PlayerId(1)).expect("player").score, 0);
    }

    #[test]
    fn dead_player_takes_no_further_damage() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        players.get_mut(&PlayerId(2)).expect("target").death_timer = Some(2.0);
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
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").score, 0);
        assert_eq!(players.get(&PlayerId(2)).expect("target").score, 0);
    }

    #[test]
    fn beam_damage_applies_player_armor() {
        let mut config = server_gameplay_config();
        config.player.armor = 0.8;
        let players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(100.0);

        let lethal = apply_player_beam_damage(&players, PlayerId(2), &mut health, 50.0, &config, false);

        assert!(!lethal);
        assert_eq!(health.0, 90.0);
    }

    #[test]
    fn beam_damage_lethal_tick_returns_true() {
        let players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(5.0);

        let lethal = apply_player_beam_damage(
            &players,
            PlayerId(2),
            &mut health,
            100.0,
            &server_gameplay_config(),
            false,
        );

        assert!(lethal);
        assert_eq!(health.0, 0.0);
    }

    #[test]
    fn dead_player_takes_no_beam_damage() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        players.get_mut(&PlayerId(2)).expect("target").death_timer = Some(2.0);
        let mut health = Health(50.0);

        let lethal = apply_player_beam_damage(
            &players,
            PlayerId(2),
            &mut health,
            100.0,
            &server_gameplay_config(),
            false,
        );

        assert!(!lethal);
        assert_eq!(health.0, 50.0);
    }

    #[test]
    fn invincible_player_takes_no_beam_damage() {
        let players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(50.0);

        let lethal = apply_player_beam_damage(
            &players,
            PlayerId(2),
            &mut health,
            100.0,
            &server_gameplay_config(),
            true,
        );

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
        let score_after_kill = players.get(&PlayerId(1)).expect("shooter").score;

        // The dying actor's entity stays queryable until removal runs later
        // in the tick; a same-tick second hit must not count as lethal again.
        let second_hit_lethal = apply_actor_projectile_hit(&mut players, &PlayerId(1), "zapper", &mut health, &config);
        assert!(!second_hit_lethal);
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").score, score_after_kill);
    }

    #[test]
    fn kill_player_broadcasts_player_death() {
        use tokio::sync::mpsc::unbounded_channel;

        let mut app = App::new();
        let mut players = PlayerMap::default();

        // Receiver with a logged-in shooter so the broadcast can reach them.
        let (shooter_tx, mut shooter_rx) = unbounded_channel();
        let mut shooter = PlayerInfo::new(Entity::PLACEHOLDER, shooter_tx);
        shooter.logged_in = true;
        players.insert(PlayerId(1), shooter);

        // The dying player; also logged_in so the broadcast targets them too.
        let mut target = make_player_info();
        target.logged_in = true;
        let target_entity = target.entity;
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
                Some(PlayerId(1)),
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
    fn kill_player_clears_state_and_arms_timer() {
        let mut app = App::new();
        let mut players = PlayerMap::default();
        let info = make_player_info();
        let entity = info.entity;
        let mut info = info;
        info.power_up_timers[common::protocol::PowerUpKind::Speed.index()] = 1.5;
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
                None,
                &mut pending_explosions,
            );
        }
        commands_queue.apply(world);

        let info = players.get(&PlayerId(7)).expect("player still tracked after death");
        assert_eq!(info.death_timer, Some(2.0));
        assert_eq!(info.power_up_timers, [0.0; common::protocol::PowerUpKind::COUNT]);
        assert!(info.held_keys.is_empty());
        assert!(info.is_dead());
    }

    #[test]
    fn clear_per_life_state_zeros_powerups_keys_and_cooldown() {
        let mut info = make_player_info();
        info.power_up_timers = [1.0; common::protocol::PowerUpKind::COUNT];
        info.stun_timer = 1.0;
        info.last_shot_time = 99.0;
        info.add_key(common::protocol::BarrierKindId(0));

        info.clear_per_life_state();

        assert_eq!(info.power_up_timers, [0.0; common::protocol::PowerUpKind::COUNT]);
        assert_eq!(info.stun_timer, 0.0);
        assert_eq!(info.last_shot_time, f32::NEG_INFINITY);
        assert!(info.held_keys.is_empty());
    }
}
