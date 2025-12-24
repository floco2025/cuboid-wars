use bevy::prelude::*;

use super::network::broadcast_to_all;
use crate::{
    constants::{SENTRY_HIT_REWARD, SENTRY_TARGET_DURATION},
    resources::{PlayerMap, SentryMap, SentryMode},
};
use common::{
    collision::{Projectile, projectile_hits_sentry, sweep_projectile_vs_player},
    constants::ALWAYS_SENTRY_HUNT,
    markers::{PlayerMarker, ProjectileMarker, SentryMarker},
    protocol::{MapLayout, *},
};

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(&Position, &FaceDirection, &PlayerId), (With<PlayerMarker>, Without<ProjectileMarker>)>,
    sentry_query: Query<(&SentryId, &Position, &FaceDirection), (With<SentryMarker>, Without<ProjectileMarker>)>,
    map_layout: Res<MapLayout>,
    mut players: ResMut<PlayerMap>,
    mut sentries: ResMut<SentryMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut projectile_query {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        // Ground bounce
        let mut hit_something = projectile.handle_ground_bounce(&proj_pos, delta).is_some_and(|new_pos| {
            *proj_pos = new_pos;
            true
        });

        // Check wall collisions
        if !hit_something {
            for wall in &map_layout.lower_walls {
                if let Some(new_pos) = projectile.handle_wall_bounce(&proj_pos, delta, wall) {
                    *proj_pos = new_pos;
                    hit_something = true;
                    break;
                }
            }
        }

        // Check roof collisions
        if !hit_something {
            for roof in &map_layout.roofs {
                if let Some(new_pos) = projectile.handle_roof_bounce(&proj_pos, delta, roof) {
                    *proj_pos = new_pos;
                    hit_something = true;
                    break;
                }
            }
        }

        // Check ramp collisions
        if !hit_something {
            for ramp in &map_layout.ramps {
                if let Some(new_pos) = projectile.handle_ramp_bounce(&proj_pos, delta, ramp) {
                    *proj_pos = new_pos;
                    hit_something = true;
                    break;
                }
            }
        }

        // If we hit a wall and despawned, skip to next projectile
        if hit_something {
            continue;
        }

        // Check sentry collisions
        for (sentry_id, sentry_pos, sentry_face_dir) in sentry_query.iter() {
            // Check collision
            if projectile_hits_sentry(&proj_pos, &projectile, delta, sentry_pos, sentry_face_dir.0) {
                let Some(sentry_info) = sentries.0.get_mut(sentry_id) else {
                    continue;
                };

                // Check if shooter has sentry hunt power-up
                let shooter_has_sentry_hunt = ALWAYS_SENTRY_HUNT
                    || players
                        .0
                        .get(shooter_id)
                        .is_some_and(|info| info.sentry_hunt_power_up_timer > 0.0);

                if shooter_has_sentry_hunt {
                    // With hunt power-up: give points and remove power-up
                    // Update shooter
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += SENTRY_HIT_REWARD;
                        shooter_info.sentry_hunt_power_up_timer = 0.0;
                    }

                    // Broadcast power-up removal to all clients
                    // Note: sentry_hunt is explicitly false (not using status()) because hitting
                    // a sentry removes the power-up even when ALWAYS_SENTRY_HUNT debug flag is on
                    if let Some(shooter_info) = players.0.get(shooter_id) {
                        let mut status = shooter_info.status(*shooter_id);
                        status.sentry_hunt_power_up = false;
                        broadcast_to_all(&players, ServerMessage::PlayerStatus(status));
                    }
                }
                // Without hunt power-up: no points, just make sentry attack

                // Always make sentry target the shooter (attack behavior)
                sentry_info.mode = SentryMode::Target;
                sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                sentry_info.follow_target = Some(*shooter_id);

                // Always despawn the projectile
                commands.entity(proj_entity).despawn();

                hit_something = true;
                break;
            }
        }

        // If we hit a sentry, skip to next projectile
        if hit_something {
            continue;
        }

        // Check player collisions
        for (position, face_direction, player_id) in player_query.iter() {
            // Use common hit detection logic
            if let Some(hit_dir) =
                sweep_projectile_vs_player(&proj_pos, &projectile, delta, position, face_direction.0)
            {
                // Self-hit: despawn without scoring to match client expectations
                if shooter_id == player_id {
                    commands.entity(proj_entity).despawn();
                    hit_something = true;
                    break;
                }

                info!("{:?} hits {:?}", shooter_id, player_id);

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
                break; // Projectile can only hit one player
            }
        }

        // If no collisions occurred, move normally
        if !hit_something {
            proj_pos.x += projectile.velocity.x * delta;
            proj_pos.y += projectile.velocity.y * delta;
            proj_pos.z += projectile.velocity.z * delta;
        }
    }
}
