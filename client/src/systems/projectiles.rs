use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use super::players::LocalPlayer;
use crate::resources::WallConfig;
use common::{
    collision::{Projectile, check_projectile_player_sweep_hit},
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
        let result = check_projectile_player_sweep_hit(projectile_pos, projectile, delta, player_pos, face_dir.0);
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

    for (projectile_entity, mut projectile_transform, mut projectile) in &mut projectile_query {
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
    let config = wall_config?;

    for wall in &config.all_walls {
        if let Some(new_pos) = projectile.handle_wall_bounce(projectile_pos, delta, wall) {
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

            if !projectile.reflects {
                // Despawn projectile without reflect power-up
                commands.entity(projectile_entity).despawn();
            }

            return Some(new_pos);
        }
    }

    None
}
