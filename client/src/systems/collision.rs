use bevy::audio::{PlaybackMode, Volume};
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;

use crate::{resources::WallConfig, systems::sync::LocalPlayer};
use common::{
    collision::{check_projectile_player_hit, check_projectile_wall_hit},
    protocol::{FaceDirection, Position},
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
    asset_server: Res<AssetServer>,
    projectile_query: Query<(Entity, &Transform, &Projectile)>,
    player_query: Query<(Entity, &Position, &FaceDirection, Has<LocalPlayer>), Without<Projectile>>,
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
                    commands.spawn((
                        AudioPlayer::new(asset_server.load("sounds/player_hits_wall.ogg")),
                        PlaybackSettings {
                            mode: PlaybackMode::Despawn,
                            volume: Volume::Linear(0.2),
                            ..default()
                        },
                    ));
                    commands.entity(proj_entity).despawn();
                    // Don't check further - projectile is already despawned
                    continue 'projectile_loop;
                }
            }
        }

        // Check player collisions
        for (_player_entity, player_pos, player_face_dir, is_local) in player_query.iter() {
            // Use common hit detection logic
            let result = check_projectile_player_hit(&proj_pos, projectile, delta, player_pos, player_face_dir.0);
            if result.hit {
                commands.spawn((
                    AudioPlayer::new(asset_server.load("sounds/player_hits_player.ogg")),
                    PlaybackSettings::DESPAWN,
                ));

                // Play hit sound if local player was hit
                if is_local {
                    commands.spawn((
                        AudioPlayer::new(asset_server.load("sounds/player_gets_hit.ogg")),
                        PlaybackSettings::DESPAWN,
                    ));
                }

                commands.entity(proj_entity).despawn();
                // Don't check further - projectile is already despawned
                continue 'projectile_loop;
            }
        }
    }
}
