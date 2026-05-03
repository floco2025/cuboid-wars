use bevy::prelude::*;

use super::network::broadcast_to_all;
use crate::{config::ServerGameplayConfig, resources::PlayerMap};
use common::{
    config::GameplayConfig,
    health::apply_damage,
    markers::{ActorMarker, PlayerMarker, ProjectileMarker},
    physics::{CollisionWorld, ProjectileMotion, projectile_hits_character},
    protocol::*,
};

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    mut player_query: Query<
        (&Position, &FaceDirection, &PlayerId, &mut Health),
        (With<PlayerMarker>, Without<ActorMarker>, Without<ProjectileMarker>),
    >,
    mut actor_query: Query<
        (&Position, &FaceDirection, &ActorId, &mut Health),
        (With<ActorMarker>, Without<PlayerMarker>, Without<ProjectileMarker>),
    >,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut projectile_query {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        // Apply gravity and air resistance
        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        // Resolve static world collisions before checking entities.
        let mut bounced = false;
        if let Some(new_pos) = projectile.resolve_world_bounces(&proj_pos, delta, &collision_world) {
            *proj_pos = new_pos;
            bounced = true;
        }

        // If we bounced off something, skip entity collision checks this frame
        if bounced {
            continue;
        }

        let mut hit_something = false;

        // Check player collisions.
        for (position, face_direction, player_id, mut health) in &mut player_query {
            if let Some(hit_dir) = projectile_hits_character(
                &proj_pos,
                &projectile,
                delta,
                position,
                face_direction.0,
                gameplay_config.characters.player.physics(),
            ) {
                // Self-hit: despawn without scoring to match client expectations
                if shooter_id == player_id {
                    commands.entity(proj_entity).despawn();
                    hit_something = true;
                    break;
                }

                info!("{:?} hits {:?}", shooter_id, player_id);
                apply_damage(&mut health, server_gameplay_config.damage.player_projectile_to_player);

                // Update hit counters in separate scopes to avoid borrow conflicts
                {
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += 1;
                    }
                }
                {
                    if let Some(target_info) = players.0.get_mut(player_id) {
                        target_info.hits -= 1;
                    }
                }

                // Broadcast hit message to all clients
                broadcast_to_all(
                    &players,
                    ServerMessage::Hit(SHit {
                        id: *player_id,
                        hit_dir_x: hit_dir.x,
                        hit_dir_z: hit_dir.z,
                    }),
                );

                // Despawn the projectile
                commands.entity(proj_entity).despawn();

                hit_something = true;
                break; // Projectile can only hit one character
            }
        }

        if !hit_something {
            for (position, face_direction, actor_id, mut health) in &mut actor_query {
                if projectile_hits_character(
                    &proj_pos,
                    &projectile,
                    delta,
                    position,
                    face_direction.0,
                    gameplay_config.characters.actor.physics(),
                )
                .is_some()
                {
                    info!("{:?} hits {:?}", shooter_id, actor_id);
                    apply_damage(&mut health, server_gameplay_config.damage.player_projectile_to_actor);
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += 1;
                    }
                    commands.entity(proj_entity).despawn();
                    hit_something = true;
                    break; // Projectile can only hit one character
                }
            }
        }

        // If no collisions occurred, move normally
        if !hit_something {
            *proj_pos += projectile.velocity * delta;
        }
    }
}
