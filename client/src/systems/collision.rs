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
    let walls = wall_config.as_deref();

    for (projectile_entity, projectile_transform, projectile) in projectile_query.iter() {
        let projectile_pos = Position {
            x: projectile_transform.translation.x,
            y: projectile_transform.translation.y,
            z: projectile_transform.translation.z,
        };

        if handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            projectile_entity,
            projectile,
            &projectile_pos,
            delta,
            walls,
        ) {
            continue;
        }

        if handle_player_collisions(
            &mut commands,
            asset_server.as_ref(),
            projectile_entity,
            projectile,
            &projectile_pos,
            delta,
            &player_query,
        ) {
            continue;
        }
    }
}

fn handle_wall_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projectile_entity: Entity,
    projectile: &Projectile,
    projectile_pos: &Position,
    delta: f32,
    wall_config: Option<&WallConfig>,
) -> bool {
    let Some(config) = wall_config else {
        return false;
    };

    for wall in &config.walls {
        if check_projectile_wall_hit(projectile_pos, projectile, delta, wall) {
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
            commands.entity(projectile_entity).despawn();
            return true;
        }
    }

    false
}

fn handle_player_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projectile_entity: Entity,
    projectile: &Projectile,
    projectile_pos: &Position,
    delta: f32,
    player_query: &Query<(Entity, &Position, &FaceDirection, Has<LocalPlayer>), Without<Projectile>>,
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
