use bevy::prelude::*;

use super::network::{broadcast_actor_destroyed, broadcast_actor_teleport};
use crate::{
    config::{ActorExplosionDamageConfig, ServerGameplayConfig},
    resources::{MapConfig, PlayerMap},
    systems::characters::generate_character_spawn_position,
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::CHARACTER_FALL_TELEPORT_Y,
    health::apply_damage,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, Health, Position},
};

pub fn actor_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut query: Query<
        (
            Entity,
            &ActorId,
            &mut Position,
            &mut CharacterVerticalVelocity,
            &ActorMoveIntent,
        ),
        With<ActorMarker>,
    >,
) {
    let fallen: Vec<(Entity, ActorId)> = query
        .iter()
        .filter_map(|(entity, id, pos, _, _)| (pos.y < CHARACTER_FALL_TELEPORT_Y).then_some((entity, *id)))
        .collect();

    if fallen.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = query.iter().map(|(_, _, pos, _, _)| *pos).collect();

    for (entity, id) in fallen {
        let teleport_pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.actor.physics(),
        );

        if let Ok((_, _, mut pos, mut motion, move_intent)) = query.get_mut(entity) {
            *pos = teleport_pos;
            motion.0 = 0.0;
            broadcast_actor_teleport(&players, id, teleport_pos, *move_intent);
        }

        occupied_positions.push(teleport_pos);
        info!("{:?} fell and teleported to {:?}", id, teleport_pos);
    }
}

pub fn actor_health_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut player_query: Query<(&Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    mut query: ActorHealthRecoveryQuery,
) {
    let destroyed: Vec<(Entity, ActorId, Position)> = query
        .iter()
        .filter_map(|(entity, id, pos, _, _, health)| (health.0 <= 0.0).then_some((entity, *id, *pos)))
        .collect();

    if destroyed.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = query.iter().map(|(_, _, pos, _, _, _)| *pos).collect();
    let max_health = gameplay_config.characters.actor.health().max;

    for (entity, id, destroyed_pos) in destroyed {
        apply_actor_explosion_damage(
            destroyed_pos,
            entity,
            &server_gameplay_config.damage.actor_explosion,
            &gameplay_config,
            &mut player_query,
            &mut query,
        );

        let respawn_pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.actor.physics(),
        );

        if let Ok((_, _, mut pos, mut motion, move_intent, mut health)) = query.get_mut(entity) {
            broadcast_actor_destroyed(&players, id, destroyed_pos);

            *pos = respawn_pos;
            motion.0 = 0.0;
            health.0 = max_health;
            broadcast_actor_teleport(&players, id, respawn_pos, *move_intent);
        }

        occupied_positions.push(respawn_pos);
        info!("{:?} was destroyed and respawned at {:?}", id, respawn_pos);
    }
}

type ActorHealthRecoveryQuery<'w, 's> = Query<
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
    actor_query: &mut ActorHealthRecoveryQuery,
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
