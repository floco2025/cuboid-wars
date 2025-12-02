use bevy::prelude::*;

use crate::{net::ServerToClient, resources::{GridConfig, PlayerMap}};
use common::{collision::{check_projectile_player_hit, check_projectile_wall_hit}, protocol::*, systems::Projectile};

// ============================================================================
// Hit Detection System
// ============================================================================

// Hit detection system - authoritative collision detection
pub fn hit_detection_system(
    mut commands: Commands,
    time: Res<Time>,
    projectile_query: Query<(Entity, &Position, &Projectile, &PlayerId)>,
    player_query: Query<(&Position, &FaceDirection, &PlayerId), Without<Projectile>>,
    grid_config: Res<GridConfig>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, proj_pos, projectile, shooter_id) in projectile_query.iter() {
        let mut hit_something = false;

        // Check wall collisions first
        for wall in &grid_config.walls {
            if check_projectile_wall_hit(proj_pos, projectile, delta, wall) {
                // Despawn the projectile when it hits a wall
                commands.entity(proj_entity).despawn();
                hit_something = true;
                break;
            }
        }

        if hit_something {
            continue; // Move to next projectile
        }

        // Check player collisions
        for (player_pos, player_face_dir, target_id) in player_query.iter() {
            // Don't hit yourself
            if shooter_id == target_id {
                continue;
            }

            // Use common hit detection logic
            let result = check_projectile_player_hit(proj_pos, projectile, delta, player_pos, player_face_dir.0);

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

                break; // Projectile can only hit one player
            }
        }
    }
}
