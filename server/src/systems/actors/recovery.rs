use bevy::prelude::*;

use super::network::broadcast_actor_destroyed;
use crate::{
    config::{ActorExplosionDamageConfig, ServerGameplayConfig},
    resources::{ActorMap, PlayerMap},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::CHARACTER_FALL_TELEPORT_Y,
    health::apply_damage,
    markers::{ActorMarker, PlayerMarker},
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, ActorMoveIntent, Health, Position},
};

// Despawn actors that have either fallen below the death threshold or had
// their health reduced to zero. Health-zero death also applies blast damage
// to nearby characters and broadcasts the explosion VFX. Falls are silent —
// they were teleports before, so the asymmetry is preserved.
//
// Actor entities are despawned outright; the `actor_spawn_quota_system` will
// pick the missing slots up next tick and create replacements.
pub fn actor_death_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    players: Res<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<(&Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    mut query: ActorDeathQuery,
) {
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, _, _, health) in query.iter() {
        if pos.y < CHARACTER_FALL_TELEPORT_Y {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                cause: DeathCause::Fall,
            });
        } else if health.0 <= 0.0 {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                cause: DeathCause::Killed,
            });
        }
    }

    if deaths.is_empty() {
        return;
    }

    for death in deaths {
        if matches!(death.cause, DeathCause::Killed) {
            apply_actor_explosion_damage(
                death.pos,
                death.entity,
                &server_gameplay_config.damage.actor_explosion,
                &gameplay_config,
                &mut player_query,
                &mut query,
            );
            broadcast_actor_destroyed(&players, death.id, death.pos);
            info!("{:?} was destroyed at {:?}", death.id, death.pos);
        } else {
            info!("{:?} fell and despawned at {:?}", death.id, death.pos);
        }
        commands.entity(death.entity).despawn();
        actors.0.remove(&death.id);
    }
}

#[derive(Copy, Clone)]
enum DeathCause {
    Fall,
    Killed,
}

struct ActorDeath {
    entity: Entity,
    id: ActorId,
    pos: Position,
    cause: DeathCause,
}

type ActorDeathQuery<'w, 's> = Query<
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

fn apply_actor_explosion_damage(
    destroyed_pos: Position,
    destroyed_entity: Entity,
    damage_config: &ActorExplosionDamageConfig,
    gameplay_config: &GameplayConfig,
    player_query: &mut Query<(&Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    actor_query: &mut ActorDeathQuery,
) {
    let actor_physics = gameplay_config.characters.actor.physics();
    let explosion_center = character_center(destroyed_pos, actor_physics);

    for (pos, mut health) in player_query.iter_mut() {
        let damage = blast_damage(
            explosion_center,
            character_center(*pos, gameplay_config.characters.player.physics()),
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
