use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use super::players::LocalPlayerMarker;
use crate::resources::PlayerMap;
use common::{
    collision::projectiles::{Projectile, projectile_hits_ghost, sweep_projectile_vs_player},
    constants::ALWAYS_GHOST_HUNT,
    markers::{GhostMarker, PlayerMarker, ProjectileMarker},
    protocol::{FaceDirection, MapLayout, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn handle_ghost_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projectile_entity: Entity,
    projectile: &Projectile,
    projectile_pos: &Position,
    shooter_id: &PlayerId,
    delta: f32,
    ghost_query: &Query<&Position, With<GhostMarker>>,
    players: &PlayerMap,
) -> bool {
    // Only check ghost collisions if shooter has ghost hunt power-up
    let Some(shooter_info) = players.0.get(shooter_id) else {
        return false;
    };

    if !ALWAYS_GHOST_HUNT && !shooter_info.ghost_hunt_power_up {
        return false;
    }

    for ghost_pos in ghost_query.iter() {
        if projectile_hits_ghost(projectile_pos, projectile, delta, ghost_pos) {
            play_sound(
                commands,
                asset_server,
                "sounds/player_hits_ghost.wav",
                PlaybackSettings::DESPAWN,
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
    player_query: &Query<(Entity, &Position, &FaceDirection, Has<LocalPlayerMarker>), With<PlayerMarker>>,
) -> bool {
    for (_player_entity, player_pos, face_dir, is_local_player) in player_query.iter() {
        let result = sweep_projectile_vs_player(projectile_pos, projectile, delta, player_pos, face_dir.0);
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
    mut projectile_query: Query<(Entity, &mut Transform, &mut Projectile, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(Entity, &Position, &FaceDirection, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    ghost_query: Query<&Position, With<GhostMarker>>,
    players: Res<PlayerMap>,
    map_layout: Option<Res<MapLayout>>,
) {
    let delta = time.delta_secs();
    let map_layout = map_layout.as_deref();

    for (projectile_entity, mut projectile_transform, mut projectile, shooter_id) in &mut projectile_query {
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
            &mut projectile,
            &projectile_pos,
            delta,
            map_layout,
        ) {
            pos_after_bounce
        } else {
            // No wall collision, check ghost collisions first
            if handle_ghost_collisions(
                &mut commands,
                asset_server.as_ref(),
                projectile_entity,
                &projectile,
                &projectile_pos,
                shooter_id,
                delta,
                &ghost_query,
                &players,
            ) {
                // Hit a ghost, projectile was despawned
                continue;
            }

            // Check player collisions
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
    projectile: &mut Projectile,
    projectile_pos: &Position,
    delta: f32,
    map_layout: Option<&MapLayout>,
) -> Option<Position> {
    let map_layout = map_layout?;

    // Ground bounce first
    if let Some(new_pos) = projectile.handle_ground_bounce(projectile_pos, delta) {
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

        return Some(new_pos);
    }

    // Check walls
    for wall in &map_layout.lower_walls {
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

            return Some(new_pos);
        }
    }

    // Check roofs
    for roof in &map_layout.roofs {
        if let Some(new_pos) = projectile.handle_roof_bounce(projectile_pos, delta, roof) {
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

            return Some(new_pos);
        }
    }

    // Check ramps
    for ramp in &map_layout.ramps {
        if let Some(new_pos) = projectile.handle_ramp_bounce(projectile_pos, delta, ramp) {
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

            return Some(new_pos);
        }
    }

    None
}
