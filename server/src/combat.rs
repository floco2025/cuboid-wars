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

pub fn apply_player_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    target_id: PlayerId,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
) {
    apply_damage(target_health, server_gameplay_config.player.projectile_damage_taken);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.hits += 1;
    }
    if let Some(target_info) = players.get_mut(&target_id) {
        target_info.hits -= 1;
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
        shooter_info.hits += 1;
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

pub fn apply_actor_explosion_damage(
    destroyed_pos: Position,
    destroyed_entity: Entity,
    destroyed_spawn_kind: &str,
    damage_config: &ActorExplosionDamageConfig,
    gameplay_config: &GameplayConfig,
    player_query: &mut Query<(&Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    actor_query: &mut ActorDeathQuery,
) {
    let actor_physics = gameplay_config.validated_actor(destroyed_spawn_kind).physics();
    let explosion_center = character_center(destroyed_pos, actor_physics);

    for (pos, mut health) in player_query.iter_mut() {
        let damage = blast_damage(
            explosion_center,
            character_center(*pos, gameplay_config.player.physics()),
            damage_config.radius,
            damage_config.player_max_damage,
        );
        apply_damage(&mut health, damage);
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
