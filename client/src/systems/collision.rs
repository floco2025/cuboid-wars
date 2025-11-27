use crate::resources::WallConfig;
use bevy::prelude::*;
use common::{
    collision::{check_projectile_player_hit, check_projectile_wall_hit},
    protocol::{Movement, Position},
    systems::Projectile,
};

// ============================================================================
// Client-Side Collision Detection
// ============================================================================

// Client-side hit detection - only for despawning projectiles visually
// Server is authoritative for actual hit scoring
pub fn client_hit_detection_system(
    mut commands: Commands,
    time: Res<Time>,
    projectile_query: Query<(Entity, &Transform, &Projectile)>,
    player_query: Query<(&Position, &Movement), Without<Projectile>>,
    wall_config: Option<Res<WallConfig>>,
) {
    let delta = time.delta_secs();

    'projectile_loop: for (proj_entity, proj_transform, projectile) in projectile_query.iter() {
        // Convert Transform to Position for hit detection
        let proj_pos = Position {
            x: proj_transform.translation.x,
            y: proj_transform.translation.y,
            z: proj_transform.translation.z,
        };

        // Check wall collisions first
        if let Some(wall_config) = wall_config.as_ref() {
            for wall in &wall_config.walls {
                if check_projectile_wall_hit(&proj_pos, projectile, delta, wall) {
                    commands.entity(proj_entity).despawn();
                    // Don't check further - projectile is already despawned
                    continue 'projectile_loop;
                }
            }
        }

        // Check player collisions
        for (player_pos, player_mov) in player_query.iter() {
            // Use common hit detection logic
            let result = check_projectile_player_hit(&proj_pos, projectile, delta, player_pos, player_mov);
            if result.hit {
                commands.entity(proj_entity).despawn();
                // Don't check further - projectile is already despawned
                continue 'projectile_loop;
            }
        }
    }
}
