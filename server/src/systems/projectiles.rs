use bevy::prelude::*;

use crate::{
    constants::GHOST_HIT_REWARD,
    resources::{GhostMap, GhostMode, PlayerMap},
};
use common::protocol::GridConfig;
use common::{
    collision::projectile::{Projectile, projectile_hits_ghost, sweep_projectile_vs_player},
    constants::ALWAYS_GHOST_HUNT,
    markers::{GhostMarker, PlayerMarker, ProjectileMarker},
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
    ghost_query: Query<(&GhostId, &Position), (With<GhostMarker>, Without<ProjectileMarker>)>,
    grid_config: Res<GridConfig>,
    mut players: ResMut<PlayerMap>,
    ghosts: Res<GhostMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut projectile_query {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        let mut hit_something = false;

        // Ground bounce
        if let Some(new_pos) = projectile.handle_ground_bounce(&proj_pos, delta) {
            if projectile.reflects {
                proj_pos.x = new_pos.x;
                proj_pos.y = new_pos.y;
                proj_pos.z = new_pos.z;
            } else {
                commands.entity(proj_entity).despawn();
            }

            hit_something = true;
        }

        // Check wall collisions
        if !hit_something {
            for wall in &grid_config.lower_walls {
                if let Some(new_pos) = projectile.handle_wall_bounce(&proj_pos, delta, wall) {
                    if projectile.reflects {
                        proj_pos.x = new_pos.x;
                        proj_pos.y = new_pos.y;
                        proj_pos.z = new_pos.z;
                    } else {
                        commands.entity(proj_entity).despawn();
                    }

                    hit_something = true;
                    break;
                }
            }
        }

        // Check roof collisions
        if !hit_something {
            for roof in &grid_config.roofs {
                if let Some(new_pos) = projectile.handle_roof_bounce(&proj_pos, delta, roof) {
                    if projectile.reflects {
                        proj_pos.x = new_pos.x;
                        proj_pos.y = new_pos.y;
                        proj_pos.z = new_pos.z;
                    } else {
                        commands.entity(proj_entity).despawn();
                    }

                    hit_something = true;
                    break;
                }
            }
        }

        // Check ramp collisions
        if !hit_something {
            for ramp in &grid_config.ramps {
                if let Some(new_pos) = projectile.handle_ramp_bounce(&proj_pos, delta, ramp) {
                    if projectile.reflects {
                        proj_pos.x = new_pos.x;
                        proj_pos.y = new_pos.y;
                        proj_pos.z = new_pos.z;
                    } else {
                        commands.entity(proj_entity).despawn();
                    }
                    hit_something = true;
                    break;
                }
            }
        }

        // If we hit a wall and despawned, skip to next projectile
        if hit_something {
            continue;
        }

        // Check ghost collisions (only for players with ghost hunt power-up who are being targeted)
        let shooter_has_ghost_hunt = ALWAYS_GHOST_HUNT
            || players
                .0
                .get(shooter_id)
                .is_some_and(|info| info.ghost_hunt_power_up_timer > 0.0);

        if shooter_has_ghost_hunt {
            for (ghost_id, ghost_pos) in ghost_query.iter() {
                let Some(ghost_info) = ghosts.0.get(ghost_id) else {
                    continue;
                };

                // Only allow hitting ghosts that are fleeing (in Target mode with ghost hunt active)
                if ghost_info.mode != GhostMode::Target {
                    continue;
                }

                // Check if the ghost is targeting the shooter (the player must be targeted to hit ghosts)
                let ghost_targets_shooter = ghost_info
                    .follow_target
                    .is_some_and(|target_id| target_id == *shooter_id);

                if !ghost_targets_shooter {
                    continue;
                }

                // Check collision
                if projectile_hits_ghost(&proj_pos, &projectile, delta, ghost_pos) {
                    // Update shooter
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += GHOST_HIT_REWARD;
                        shooter_info.ghost_hunt_power_up_timer = 0.0;
                    }

                    // Broadcast power-up removal to all clients (we just set timer to 0 above)
                    if let Some(shooter_info) = players.0.get(shooter_id) {
                        broadcast_to_all(
                            &players,
                            ServerMessage::PlayerStatus(SPlayerStatus {
                                id: *shooter_id,
                                speed_power_up: shooter_info.speed_power_up_timer > 0.0,
                                multi_shot_power_up: shooter_info.multi_shot_power_up_timer > 0.0,
                                reflect_power_up: shooter_info.reflect_power_up_timer > 0.0,
                                phasing_power_up: shooter_info.phasing_power_up_timer > 0.0,
                                ghost_hunt_power_up: false,
                                stunned: shooter_info.stun_timer > 0.0,
                            }),
                        );
                    }

                    // Despawn the projectile
                    commands.entity(proj_entity).despawn();

                    hit_something = true;
                    break;
                }
            }
        }

        // If we hit a ghost, skip to next projectile
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
