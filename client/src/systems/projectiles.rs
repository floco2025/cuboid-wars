use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use crate::{markers::LocalPlayerMarker, resources::PlayerMap};
use common::{
    collision::{ProjectileMotion, projectile_hits_sentry, sweep_projectile_vs_player},
    constants::ALWAYS_SENTRY_HUNT,
    markers::{PlayerMarker, ProjectileMarker, SentryMarker},
    protocol::{FaceDirection, MapLayout, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn handle_sentry_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    proj_entity: Entity,
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    shooter_id: &PlayerId,
    delta: f32,
    sentry_query: &Query<(&Position, &FaceDirection), With<SentryMarker>>,
    players: &PlayerMap,
) -> bool {
    // Always check sentry collisions
    for (sentry_pos, sentry_face_dir) in sentry_query.iter() {
        if projectile_hits_sentry(proj_pos, proj_motion, delta, sentry_pos, sentry_face_dir.0) {
            // Check if shooter has sentry hunt power-up
            let shooter_has_hunt = players
                .0
                .get(shooter_id)
                .is_some_and(|info| ALWAYS_SENTRY_HUNT || info.sentry_hunt_power_up);

            if shooter_has_hunt {
                // With hunt power-up: play sentry hit sound
                play_sound(
                    commands,
                    asset_server,
                    "sounds/player_hits_sentry.wav",
                    PlaybackSettings::DESPAWN,
                );
            } else {
                // Without hunt power-up: play wall hit sound
                play_sound(
                    commands,
                    asset_server,
                    "sounds/projectile_hits_sentry_no_damage.ogg",
                    PlaybackSettings {
                        mode: PlaybackMode::Despawn,
                        volume: Volume::Linear(0.2),
                        ..default()
                    },
                );
            }

            commands.entity(proj_entity).despawn();
            return true;
        }
    }

    false
}

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
    sentry_query: Query<(&Position, &FaceDirection), With<SentryMarker>>,
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
        ) {
            pos_after_bounce
        } else {
            // No wall collision, check sentry collisions first
            if handle_sentry_collisions(
                &mut commands,
                asset_server.as_ref(),
                projectile_entity,
                &projectile,
                &projectile_pos,
                shooter_id,
                delta,
                &sentry_query,
                &players,
            ) {
                // Hit a sentry, projectile was despawned
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
) -> Option<Position> {
    let map_layout = map_layout?;

    let mut result_pos: Option<Position> = None;

    // Check walls, roofs, ramps, and ground together to catch junction corner cases.
    if let Some(new_pos) = proj_motion.handle_bounces(
        proj_pos,
        delta,
        &map_layout.lower_walls,
        &map_layout.roofs,
        &map_layout.ramps,
    ) {
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
        result_pos = Some(new_pos);
    }

    result_pos
}
