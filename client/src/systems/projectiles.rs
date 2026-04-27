use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use crate::{
    constants::{PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND, PROJECTILE_MIN_BOUNCE_SOUND_SPEED},
    markers::LocalPlayerMarker,
    resources::LastBounceSoundTime,
};
use common::{
    markers::{PlayerMarker, ProjectileMarker},
    physics::{ProjectileMotion, sweep_projectile_vs_player},
    protocol::{FaceDirection, MapLayout, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn handle_player_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    proj_entity: Entity,
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    player_query: &Query<(Entity, &Position, &FaceDirection, Has<LocalPlayerMarker>), With<PlayerMarker>>,
) -> bool {
    for (_player_entity, player_pos, face_dir, is_local_player) in player_query.iter() {
        if sweep_projectile_vs_player(proj_pos, proj_motion, delta, player_pos, face_dir.0).is_some() {
            play_sound(
                commands,
                asset_server,
                "sounds/projectile_hits_player.ogg",
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

            commands.entity(proj_entity).despawn();
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
    mut projectile_query: Query<(Entity, &mut Transform, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(Entity, &Position, &FaceDirection, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    map_layout: Option<Res<MapLayout>>,
    mut last_bounce_sound: ResMut<LastBounceSoundTime>,
) {
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let map_layout = map_layout.as_deref();

    for (projectile_entity, mut projectile_transform, mut projectile, _shooter_id) in &mut projectile_query {
        // Check lifetime and despawn if expired
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        // Apply gravity and air resistance
        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        let projectile_pos: Position = projectile_transform.translation.into();

        // Check wall collisions and handle bouncing/despawning
        let new_pos = if let Some(pos_after_bounce) = handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            &mut projectile,
            &projectile_pos,
            delta,
            map_layout,
            current_time,
            &mut last_bounce_sound,
        ) {
            pos_after_bounce
        } else {
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
        projectile_transform.translation = new_pos.into();
    }
}

fn handle_wall_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    proj_motion: &mut ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    map_layout: Option<&MapLayout>,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSoundTime,
) -> Option<Position> {
    let map_layout = map_layout?;

    // Check speed before bounce to decide if we should play sound
    let speed_before = proj_motion.velocity.length();

    let new_pos = proj_motion.handle_bounces(
        proj_pos,
        delta,
        &map_layout.walls,
        &map_layout.floors,
        &map_layout.ramps,
    )?;

    // Play sound if speed is high enough and rate limit allows
    let min_interval = 1.0 / PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND;
    if speed_before >= PROJECTILE_MIN_BOUNCE_SOUND_SPEED && current_time - last_bounce_sound.0 >= min_interval {
        play_sound(
            commands,
            asset_server,
            "sounds/projectile_hits_wall.ogg",
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::Linear(0.2),
                ..default()
            },
        );
        last_bounce_sound.0 = current_time;
    }

    Some(new_pos)
}
