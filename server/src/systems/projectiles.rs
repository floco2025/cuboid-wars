use bevy::prelude::*;

use super::network::broadcast_to_all;
use crate::resources::PlayerMap;
use common::{
    markers::{PlayerMarker, ProjectileMarker},
    physics::{ProjectileMotion, sweep_projectile_vs_player},
    protocol::{MapLayout, *},
};

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(&Position, &FaceDirection, &PlayerId), (With<PlayerMarker>, Without<ProjectileMarker>)>,
    map_layout: Res<MapLayout>,
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

        // Check wall collisions
        let mut bounced = false;
        if let Some(new_pos) = projectile.handle_bounces(
            &proj_pos,
            delta,
            &map_layout.walls,
            &map_layout.floors,
            &map_layout.ramps,
        ) {
            *proj_pos = new_pos;
            bounced = true;
        }

        // If we bounced off something, skip entity collision checks this frame
        if bounced {
            continue;
        }

        let mut hit_something = false;

        // Check player collisions
        for (position, face_direction, player_id) in player_query.iter() {
            // Use common hit detection logic
            if let Some(hit_dir) = sweep_projectile_vs_player(&proj_pos, &projectile, delta, position, face_direction.0)
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
            *proj_pos += projectile.velocity * delta;
        }
    }
}
