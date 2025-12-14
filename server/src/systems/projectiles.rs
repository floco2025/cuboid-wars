use bevy::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{GridConfig, PlayerMap},
};
use common::{
    collision::{Projectile, check_projectile_player_sweep_hit},
    protocol::*,
};

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
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile, &PlayerId)>,
    player_query: Query<PlayerTarget, Without<Projectile>>,
    grid_config: Res<GridConfig>,
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

        let mut hit_something = false;

        // Check wall collisions first
        for wall in &grid_config.walls {
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

        // If we hit a wall and despawned, skip to next projectile
        if hit_something {
            continue;
        }

        // Check player collisions
        for player in player_query.iter() {
            // Use common hit detection logic
            let result = check_projectile_player_sweep_hit(
                &proj_pos,
                &projectile,
                delta,
                player.position,
                player.face_direction.0,
            );

            if result.hit {
                // Self-hit: despawn without scoring to match client expectations
                if shooter_id == player.player_id {
                    commands.entity(proj_entity).despawn();
                    hit_something = true;
                    break;
                }

                info!("{:?} hits {:?}", shooter_id, player.player_id);

                // Update hit counters
                if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                    shooter_info.hits += 1;
                }
                if let Some(target_info) = players.0.get_mut(player.player_id) {
                    target_info.hits -= 1;
                }

                // Broadcast hit message to all clients
                for player_info in players.0.values() {
                    let _ = player_info.channel.send(ServerToClient::Send(ServerMessage::Hit(SHit {
                        id: *player.player_id,
                        hit_dir_x: result.hit_dir_x,
                        hit_dir_z: result.hit_dir_z,
                    })));
                }

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
