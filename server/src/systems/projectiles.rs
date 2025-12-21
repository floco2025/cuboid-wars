use bevy::prelude::*;

use crate::{
    constants::{SENTRY_HIT_REWARD, SENTRY_TARGET_DURATION},
    resources::{PlayerMap, SentryMap, SentryMode},
};
use common::protocol::MapLayout;
use common::{
    collision::projectiles::{Projectile, projectile_hits_sentry, sweep_projectile_vs_player},
    constants::ALWAYS_SENTRY_HUNT,
    markers::{PlayerMarker, ProjectileMarker, SentryMarker},
    protocol::*,
};

use super::network::broadcast_to_all;

// ============================================================================
// Query Bundles
// ============================================================================

// Common query for player target (used in projectile collision)
#[derive(bevy::ecs::query::QueryData)]
pub struct PlayerTarget {
    pub position: &'static Position,
    pub face_direction: &'static FaceDirection,
    pub player_id: &'static PlayerId,
}

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<PlayerTarget, (With<PlayerMarker>, Without<ProjectileMarker>)>,
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
        let mut hit_something = if let Some(new_pos) = projectile.handle_ground_bounce(&proj_pos, delta) {
            proj_pos.x = new_pos.x;
            proj_pos.y = new_pos.y;
            proj_pos.z = new_pos.z;
            true
        } else {
            false
        };

        // Check wall collisions
        if !hit_something {
            for wall in &map_layout.lower_walls {
                if let Some(new_pos) = projectile.handle_wall_bounce(&proj_pos, delta, wall) {
                    proj_pos.x = new_pos.x;
                    proj_pos.y = new_pos.y;
                    proj_pos.z = new_pos.z;
                    hit_something = true;
                    break;
                }
            }
        }

        // Check roof collisions
        if !hit_something {
            for roof in &map_layout.roofs {
                if let Some(new_pos) = projectile.handle_roof_bounce(&proj_pos, delta, roof) {
                    proj_pos.x = new_pos.x;
                    proj_pos.y = new_pos.y;
                    proj_pos.z = new_pos.z;
                    hit_something = true;
                    break;
                }
            }
        }

        // Check ramp collisions
        if !hit_something {
            for ramp in &map_layout.ramps {
                if let Some(new_pos) = projectile.handle_ramp_bounce(&proj_pos, delta, ramp) {
                    proj_pos.x = new_pos.x;
                    proj_pos.y = new_pos.y;
                    proj_pos.z = new_pos.z;
                    hit_something = true;
                    break;
                }
            }
        }

        // If we hit a wall and despawned, skip to next projectile
        if hit_something {
            continue;
        }

        // Check sentry collisions - projectiles always hit sentries
        for (sentry_id, sentry_pos, sentry_face_dir) in sentry_query.iter() {
            // Check collision
            if projectile_hits_sentry(&proj_pos, &projectile, delta, sentry_pos, sentry_face_dir.0) {
                // Check if shooter has sentry hunt power-up
                let shooter_has_sentry_hunt = ALWAYS_SENTRY_HUNT
                    || players
                        .0
                        .get(shooter_id)
                        .is_some_and(|info| info.sentry_hunt_power_up_timer > 0.0);

                if shooter_has_sentry_hunt {
                    // With hunt power-up: give points, remove power-up, make sentry attack
                    let Some(sentry_info) = sentries.0.get_mut(sentry_id) else {
                        continue;
                    };

                    // Update shooter
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += SENTRY_HIT_REWARD;
                        shooter_info.sentry_hunt_power_up_timer = 0.0;
                    }

                    // Broadcast power-up removal to all clients
                    if let Some(shooter_info) = players.0.get(shooter_id) {
                        broadcast_to_all(
                            &players,
                            ServerMessage::PlayerStatus(SPlayerStatus {
                                id: *shooter_id,
                                speed_power_up: shooter_info.speed_power_up_timer > 0.0,
                                multi_shot_power_up: shooter_info.multi_shot_power_up_timer > 0.0,
                                phasing_power_up: shooter_info.phasing_power_up_timer > 0.0,
                                sentry_hunt_power_up: false,
                                stunned: shooter_info.stun_timer > 0.0,
                            }),
                        );
                    }

                    // Make sentry target the shooter (attack behavior)
                    sentry_info.mode = SentryMode::Target;
                    sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                    sentry_info.follow_target = Some(*shooter_id);
                }
                // Without hunt power-up: just despawn projectile (no points, no behavior change)

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
        for player in player_query.iter() {
            // Use common hit detection logic
            let result =
                sweep_projectile_vs_player(&proj_pos, &projectile, delta, player.position, player.face_direction.0);

            if result.hit {
                // Self-hit: despawn without scoring to match client expectations
                if shooter_id == player.player_id {
                    commands.entity(proj_entity).despawn();
                    hit_something = true;
                    break;
                }

                info!("{:?} hits {:?}", shooter_id, player.player_id);

                // Update hit counters in separate scopes to avoid borrow conflicts
                {
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += 1;
                    }
                }
                {
                    if let Some(target_info) = players.0.get_mut(player.player_id) {
                        target_info.hits -= 1;
                    }
                }

                // Broadcast hit message to all clients
                broadcast_to_all(
                    &players,
                    ServerMessage::Hit(SHit {
                        id: *player.player_id,
                        hit_dir_x: result.hit_dir_x,
                        hit_dir_z: result.hit_dir_z,
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
