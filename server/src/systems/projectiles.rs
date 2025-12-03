use bevy::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{GridConfig, PlayerMap},
};
use common::{
    collision::{Projectile, check_projectile_player_hit},
    protocol::*,
};

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile, &PlayerId)>,
    player_query: Query<(&Position, &FaceDirection, &PlayerId), Without<Projectile>>,
    grid_config: Res<GridConfig>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in projectile_query.iter_mut() {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        let mut hit_something = false;

        // Check wall collisions first
        for wall in &grid_config.walls {
            if let Some((normal_x, normal_z, t_collision)) = common::collision::check_projectile_wall_hit_with_normal(&proj_pos, &projectile, delta, wall) {
                if projectile.reflects {
                    info!("Projectile from {:?} bouncing off wall (has reflect power-up)", shooter_id);
                    // Move projectile to collision point
                    let collision_x = projectile.velocity.x.mul_add(delta * t_collision, proj_pos.x);
                    let collision_y = projectile.velocity.y.mul_add(delta * t_collision, proj_pos.y);
                    let collision_z = projectile.velocity.z.mul_add(delta * t_collision, proj_pos.z);
                    
                    // Reflect velocity off the wall normal
                    let dot = projectile.velocity.x.mul_add(normal_x, projectile.velocity.z * normal_z);
                    projectile.velocity.x -= 2.0 * dot * normal_x;
                    projectile.velocity.z -= 2.0 * dot * normal_z;
                    
                    // Continue moving for remaining time after bounce
                    let remaining_time = delta * (1.0 - t_collision);
                    proj_pos.x = projectile.velocity.x.mul_add(remaining_time, collision_x);
                    proj_pos.y = projectile.velocity.y.mul_add(remaining_time, collision_y);
                    proj_pos.z = projectile.velocity.z.mul_add(remaining_time, collision_z);
                } else {
                    info!("Projectile from {:?} hit wall without reflect power-up, despawning", shooter_id);
                    // Despawn projectile without reflect power-up
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
        for (player_pos, player_face_dir, target_id) in player_query.iter() {
            // Don't hit yourself
            if shooter_id == target_id {
                continue;
            }

            // Use common hit detection logic
            let result = check_projectile_player_hit(&proj_pos, &projectile, delta, player_pos, player_face_dir.0);

            if result.hit {
                info!("{:?} hits {:?}", shooter_id, target_id);

                // Update hit counters
                if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                    shooter_info.hits += 1;
                    info!("  {:?} now has {} hits", shooter_id, shooter_info.hits);
                }
                if let Some(target_info) = players.0.get_mut(target_id) {
                    target_info.hits -= 1;
                    info!("  {:?} now has {} hits", target_id, target_info.hits);
                }

                // Broadcast hit message to all clients
                for player_info in players.0.values() {
                    let _ = player_info.channel.send(ServerToClient::Send(ServerMessage::Hit(SHit {
                        id: *target_id,
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
