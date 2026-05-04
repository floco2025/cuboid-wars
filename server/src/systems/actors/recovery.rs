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
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, Health, PlayerMarker, Position},
};

// Despawn actors that have either fallen below the death threshold or had
// their health reduced to zero. Health-zero death also applies blast damage
// to nearby characters and broadcasts the explosion VFX. Falls are silent —
// they were teleports before, so the asymmetry is preserved.
//
// Actor entities are despawned outright; the `actor_respawn_system` will
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
        let Some(info) = actors.0.get(id) else {
            continue;
        };
        let spawn_kind = info.spawn_kind.clone();
        if pos.y < CHARACTER_FALL_TELEPORT_Y {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                spawn_kind,
                cause: DeathCause::Fall,
            });
        } else if health.0 <= 0.0 {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                spawn_kind,
                cause: DeathCause::Killed,
            });
        }
    }

    if deaths.is_empty() {
        return;
    }

    for death in deaths {
        if matches!(death.cause, DeathCause::Killed) {
            // Per-kind config: cross-validated at startup, so unwrap is safe.
            let kind_server_config = server_gameplay_config
                .actor(&death.spawn_kind)
                .expect("actor kind validated at startup");
            apply_actor_explosion_damage(
                death.pos,
                death.entity,
                &death.spawn_kind,
                &kind_server_config.explosion,
                &gameplay_config,
                &mut player_query,
                &mut query,
            );
            broadcast_actor_destroyed(&players, death.id, death.pos);
            info!("{:?} was destroyed at {:?}", death.id, death.pos);
        } else {
            info!("{:?} fell and despawned at {:?}", death.id, death.pos);
        }
        // Death is unconditional despawn. For respawning kinds, the throttle
        // in `actor_respawn_system` ticks down only while the slot is short
        // of its quota, so dropping a live actor here is what causes the
        // next replacement to be scheduled. For non-respawning kinds, the
        // respawn system skips the zone entirely — no replacement happens.
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
    spawn_kind: String,
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
    destroyed_spawn_kind: &str,
    damage_config: &ActorExplosionDamageConfig,
    gameplay_config: &GameplayConfig,
    player_query: &mut Query<(&Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    actor_query: &mut ActorDeathQuery,
) {
    let actor_physics = gameplay_config
        .actor(destroyed_spawn_kind)
        .expect("actor kind validated at startup")
        .physics();
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
