use bevy::prelude::*;

use crate::{
    config::{ActorExplosionDamageConfig, ServerGameplayConfig},
    resources::PlayerMap,
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    health::apply_damage,
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, Health, PlayerId, PlayerMarker, Position},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerHitOutcome {
    Survived,
    Died,
}

// Common death sequence: clear per-life state, arm the respawn timer,
// despawn the entity. Called from every code path that takes a player to
// zero health (projectile hits, actor explosions, falls).
pub fn kill_player(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    entity: Entity,
    respawn_delay_secs: f32,
) {
    if let Some(info) = players.get_mut(&id) {
        info.clear_per_life_state();
        info.death_timer = Some(respawn_delay_secs);
    }
    commands.entity(entity).despawn();
}

// Apply one projectile hit to a player. Returns `Died` when this hit drops
// the target's health to zero (and the target wasn't already dead). The
// caller is responsible for despawning the target's entity and arming the
// respawn timer.
pub fn apply_player_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    target_id: PlayerId,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
) -> PlayerHitOutcome {
    // Defensive: a dead player has no entity, so the projectile system
    // shouldn't find them — but guard anyway so a stray hit can't redeath them.
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return PlayerHitOutcome::Survived;
    }

    apply_damage(target_health, server_gameplay_config.player.projectile_damage_taken);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.score += 1;
    }
    if let Some(target_info) = players.get_mut(&target_id) {
        target_info.score -= 1;
    }

    if target_health.0 <= 0.0 {
        PlayerHitOutcome::Died
    } else {
        PlayerHitOutcome::Survived
    }
}

pub fn apply_actor_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    target_health: &mut Health,
    actor_kind: &str,
    server_gameplay_config: &ServerGameplayConfig,
) {
    let damage = server_gameplay_config
        .validated_actor(actor_kind)
        .combat
        .projectile_damage_taken;
    apply_damage(target_health, damage);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.score += 1;
    }
}

pub type ActorDeathQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static ActorMoveIntent,
        &'static mut Health,
    ),
    With<ActorMarker>,
>;

// Applies blast damage to nearby players and actors. Returns the
// `(PlayerId, Entity)` of any player whose health dropped to zero on this
// call so the caller can run the standard death/respawn flow.
//
// Players already in the death state (`death_timer.is_some()`) are skipped
// — their entity is queued for despawn this tick and we don't want a stray
// explosion to "kill" them again.
pub fn apply_actor_explosion_damage(
    destroyed_pos: Position,
    destroyed_entity: Entity,
    destroyed_spawn_kind: &str,
    damage_config: &ActorExplosionDamageConfig,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    player_query: &mut Query<
        (Entity, &PlayerId, &Position, &mut Health),
        (With<PlayerMarker>, Without<ActorMarker>),
    >,
    actor_query: &mut ActorDeathQuery,
) -> Vec<(PlayerId, Entity)> {
    let actor_physics = gameplay_config.validated_actor(destroyed_spawn_kind).physics();
    let explosion_center = character_center(destroyed_pos, actor_physics);
    let mut newly_dead = Vec::new();

    for (entity, id, pos, mut health) in player_query.iter_mut() {
        if players.get(id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        let damage = blast_damage(
            explosion_center,
            character_center(*pos, gameplay_config.player.physics()),
            damage_config.radius,
            damage_config.player_max_damage,
        );
        if damage <= 0.0 {
            continue;
        }
        apply_damage(&mut health, damage);
        if health.0 <= 0.0 {
            newly_dead.push((*id, entity));
        }
    }

    for (entity, _, pos, _, _, mut health) in actor_query.iter_mut() {
        if entity == destroyed_entity {
            continue;
        }

        let damage = blast_damage(
            explosion_center,
            character_center(*pos, actor_physics),
            damage_config.radius,
            damage_config.actor_max_damage,
        );
        apply_damage(&mut health, damage);
    }

    newly_dead
}

fn blast_damage(center: Vec3, target: Vec3, radius: f32, max_damage: f32) -> f32 {
    let distance = center.distance(target);
    if distance > radius {
        return 0.0;
    }

    max_damage * (1.0 - distance / radius)
}

fn character_center(pos: Position, physics: CharacterPhysicsConfig) -> Vec3 {
    Vec3::new(pos.x, physics.collider_center_y(pos.y), pos.z)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::PlayerServerConfig;
    use crate::resources::PlayerInfo;
    use tokio::sync::mpsc::unbounded_channel;

    fn server_gameplay_config() -> ServerGameplayConfig {
        ServerGameplayConfig {
            version: 1,
            player: PlayerServerConfig {
                projectile_damage_taken: 25.0,
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

        let outcome = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
        );

        assert_eq!(outcome, PlayerHitOutcome::Survived);
        assert_eq!(health.0, 75.0);
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").score, 1);
        assert_eq!(players.get(&PlayerId(2)).expect("target").score, -1);
    }

    #[test]
    fn lethal_hit_returns_died() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        let mut health = Health(10.0);

        let outcome = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
        );

        assert_eq!(outcome, PlayerHitOutcome::Died);
        assert_eq!(health.0, 0.0);
    }

    #[test]
    fn dead_player_takes_no_further_damage() {
        let mut players = make_player_map_with(PlayerId(1), PlayerId(2));
        players.get_mut(&PlayerId(2)).expect("target").death_timer = Some(2.0);
        let mut health = Health(0.0);

        let outcome = apply_player_projectile_hit(
            &mut players,
            &PlayerId(1),
            PlayerId(2),
            &mut health,
            &server_gameplay_config(),
        );

        assert_eq!(outcome, PlayerHitOutcome::Survived);
        // Score must not move on a no-op hit.
        assert_eq!(players.get(&PlayerId(1)).expect("shooter").score, 0);
        assert_eq!(players.get(&PlayerId(2)).expect("target").score, 0);
    }

    #[test]
    fn kill_player_clears_state_and_arms_timer() {
        let mut app = App::new();
        let mut players = PlayerMap::default();
        let info = make_player_info();
        let entity = info.entity;
        let mut info = info;
        info.speed_power_up_timer = 1.5;
        info.add_key(common::protocol::BarrierKindId(0));
        players.insert(PlayerId(7), info);

        let world = app.world_mut();
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();
        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            kill_player(&mut commands, &mut players, PlayerId(7), entity, 2.0);
        }
        commands_queue.apply(world);

        let info = players.get(&PlayerId(7)).expect("player still tracked after death");
        assert_eq!(info.death_timer, Some(2.0));
        assert_eq!(info.speed_power_up_timer, 0.0);
        assert!(info.held_keys.is_empty());
        assert!(info.is_dead());
    }

    #[test]
    fn clear_per_life_state_zeros_powerups_and_keys() {
        let mut info = make_player_info();
        info.speed_power_up_timer = 1.0;
        info.multi_shot_power_up_timer = 1.0;
        info.phasing_power_up_timer = 1.0;
        info.anti_gravity_power_up_timer = 1.0;
        info.stun_timer = 1.0;
        info.add_key(common::protocol::BarrierKindId(0));

        info.clear_per_life_state();

        assert_eq!(info.speed_power_up_timer, 0.0);
        assert_eq!(info.multi_shot_power_up_timer, 0.0);
        assert_eq!(info.phasing_power_up_timer, 0.0);
        assert_eq!(info.anti_gravity_power_up_timer, 0.0);
        assert_eq!(info.stun_timer, 0.0);
        assert!(info.held_keys.is_empty());
    }
}
