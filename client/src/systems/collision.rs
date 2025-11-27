use bevy::prelude::*;
use common::protocol::{Movement, Position};
use common::systems::Projectile;
use crate::resources::WallConfig;

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

    for (proj_entity, proj_transform, projectile) in projectile_query.iter() {
        // Convert Transform to Position for hit detection
        let proj_pos = Position {
            x: proj_transform.translation.x,
            y: proj_transform.translation.y,
            z: proj_transform.translation.z,
        };

        // Check wall collisions first
        if let Some(wall_config) = wall_config.as_ref() {
            for wall in &wall_config.walls {
                if common::collision::check_projectile_wall_hit(&proj_pos, projectile, delta, wall) {
                    commands.entity(proj_entity).despawn();
                    // Don't check further - projectile is already despawned
                    continue;
                }
            }
        }

        // Check player collisions
        for (player_pos, player_mov) in player_query.iter() {
            // Use common hit detection logic
            let result = common::collision::check_projectile_hit(&proj_pos, projectile, delta, player_pos, player_mov);
            
            if result.hit {
                // Only despawn the projectile visually - server handles scoring
                commands.entity(proj_entity).despawn();
                break; // Projectile can only hit one player
            }
        }
    }
}
