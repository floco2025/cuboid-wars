use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileMarker, ProjectileMotion},
    protocol::{ActorId, ActorMarker, FaceDirection, PlayerId, PlayerMarker, Position},
};

use super::{
    audio::LastBounceSoundTime,
    collision::{handle_barrier_collisions, handle_character_collisions, handle_wall_collisions},
};
use crate::{actors::ActorMap, config::AssetSet, players::LocalPlayerMarker};

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    mut projectile_query: Query<(Entity, &mut Transform, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(Entity, &Position, &FaceDirection, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position, &FaceDirection), With<ActorMarker>>,
    actors: Res<ActorMap>,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
    mut last_bounce_sound: ResMut<LastBounceSoundTime>,
) {
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let collision_world = collision_world.as_deref();

    for (projectile_entity, mut projectile_transform, mut projectile, shooter_id) in &mut projectile_query {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        let projectile_pos: Position = projectile_transform.translation.into();

        if handle_barrier_collisions(
            &mut commands,
            asset_server.as_ref(),
            &asset_set,
            projectile_entity,
            &projectile,
            &projectile_pos,
            delta,
            collision_world,
        ) {
            continue;
        }

        let new_pos = if let Some(pos_after_bounce) = handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            &asset_set,
            &mut projectile,
            &projectile_pos,
            delta,
            collision_world,
            current_time,
            &mut last_bounce_sound,
        ) {
            pos_after_bounce
        } else {
            if handle_character_collisions(
                &mut commands,
                asset_server.as_ref(),
                &asset_set,
                projectile_entity,
                &projectile,
                &projectile_pos,
                delta,
                *shooter_id,
                &player_query,
                &actor_query,
                &actors,
                &gameplay_config,
            ) {
                continue;
            }

            Position {
                x: projectile.velocity.x.mul_add(delta, projectile_pos.x),
                y: projectile.velocity.y.mul_add(delta, projectile_pos.y),
                z: projectile.velocity.z.mul_add(delta, projectile_pos.z),
            }
        };

        projectile_transform.translation = new_pos.into();
    }
}
