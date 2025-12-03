use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use super::players::LocalPlayer;
use crate::resources::WallConfig;
use common::{
    collision::{Projectile, check_projectile_player_hit},
    protocol::{FaceDirection, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn handle_player_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projectile_entity: Entity,
    projectile: &Projectile,
    projectile_pos: &Position,
    delta: f32,
    player_query: &Query<(Entity, &Position, &FaceDirection, Has<LocalPlayer>), With<PlayerId>>,
) -> bool {
    for (_player_entity, player_pos, face_dir, is_local_player) in player_query.iter() {
        let result = check_projectile_player_hit(projectile_pos, projectile, delta, player_pos, face_dir.0);
        if result.hit {
            play_sound(
                commands,
                asset_server,
                "sounds/player_hits_player.ogg",
                PlaybackSettings::DESPAWN,
            );

            if is_local_player {
                play_sound(
                    commands,
                    asset_server,
                    "sounds/player_gets_hit.ogg",
                    PlaybackSettings::DESPAWN,
                );
            }

            commands.entity(projectile_entity).despawn();
            return true;
        }
    }

    false
}

fn play_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &'static str,
    settings: PlaybackSettings,
) {
    commands.spawn((AudioPlayer::new(asset_server.load(asset_path)), settings));
}

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mut projectile_query: Query<(Entity, &mut Transform, &mut Projectile)>,
    player_query: Query<(Entity, &Position, &FaceDirection, Has<LocalPlayer>), With<PlayerId>>,
    wall_config: Option<Res<WallConfig>>,
) {
    let delta = time.delta_secs();
    let walls = wall_config.as_deref();

    for (projectile_entity, mut projectile_transform, mut projectile) in projectile_query.iter_mut() {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        let projectile_pos = Position {
            x: projectile_transform.translation.x,
            y: projectile_transform.translation.y,
            z: projectile_transform.translation.z,
        };

        // Check wall collisions and handle bouncing/despawning
        let new_pos = if let Some(pos_after_bounce) = handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            projectile_entity,
            &mut projectile,
            &projectile_pos,
            delta,
            walls,
        ) {
            pos_after_bounce
        } else {
            // No wall collision, check player collisions
            if handle_player_collisions(
                &mut commands,
                asset_server.as_ref(),
                projectile_entity,
                &projectile,
                &projectile_pos,
                delta,
                &player_query,
            ) {
                // Hit a player, projectile was despawned
                continue;
            }
            
            // No collisions, move normally
            Position {
                x: projectile.velocity.x.mul_add(delta, projectile_pos.x),
                y: projectile.velocity.y.mul_add(delta, projectile_pos.y),
                z: projectile.velocity.z.mul_add(delta, projectile_pos.z),
            }
        };
        
        // Update transform to new position
        projectile_transform.translation.x = new_pos.x;
        projectile_transform.translation.y = new_pos.y;
        projectile_transform.translation.z = new_pos.z;
    }
}

fn handle_wall_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projectile_entity: Entity,
    projectile: &mut Projectile,
    projectile_pos: &Position,
    delta: f32,
    wall_config: Option<&WallConfig>,
) -> Option<Position> {
    let Some(config) = wall_config else {
        return None;
    };

    for wall in &config.walls {
        if let Some((normal_x, normal_z, t_collision)) = common::collision::check_projectile_wall_hit_with_normal(projectile_pos, projectile, delta, wall) {
            play_sound(
                commands,
                asset_server,
                "sounds/player_hits_wall.ogg",
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(0.2),
                    ..default()
                },
            );
            
            if projectile.reflects {
                // Move projectile to collision point
                let collision_x = projectile.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = projectile.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = projectile.velocity.z.mul_add(delta * t_collision, projectile_pos.z);
                
                // Reflect velocity off the wall normal
                let dot = projectile.velocity.x.mul_add(normal_x, projectile.velocity.z * normal_z);
                projectile.velocity.x -= 2.0 * dot * normal_x;
                projectile.velocity.z -= 2.0 * dot * normal_z;
                
                // Continue moving for remaining time after bounce
                let remaining_time = delta * (1.0 - t_collision);
                let new_pos = Position {
                    x: projectile.velocity.x.mul_add(remaining_time, collision_x),
                    y: projectile.velocity.y.mul_add(remaining_time, collision_y),
                    z: projectile.velocity.z.mul_add(remaining_time, collision_z),
                };
                
                return Some(new_pos);
            } else {
                // Despawn projectile without reflect power-up
                commands.entity(projectile_entity).despawn();
                return Some(*projectile_pos); // Return current position (will be despawned anyway)
            }
        }
    }

    None
}
