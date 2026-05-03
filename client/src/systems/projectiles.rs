use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use crate::{
    config::AssetSet,
    constants::{PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND, PROJECTILE_MIN_BOUNCE_SOUND_SPEED},
    markers::LocalPlayerMarker,
    resources::{ActorMap, LastBounceSoundTime},
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker, ProjectileMarker},
    physics::{CollisionWorld, ProjectileCharacterHit, ProjectileMotion, projectile_character_hit},
    protocol::{ActorId, FaceDirection, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn handle_character_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    proj_entity: Entity,
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    shooter_id: PlayerId,
    player_query: &Query<(Entity, &Position, &FaceDirection, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: &Query<(&ActorId, &Position, &FaceDirection), With<ActorMarker>>,
    actors: &ActorMap,
    gameplay_config: &GameplayConfig,
) -> bool {
    let mut closest_hit = None;

    for (_player_entity, player_pos, face_dir, player_id, is_local_player) in player_query.iter() {
        if shooter_id == *player_id {
            continue;
        }

        if let Some(hit) = projectile_character_hit(
            proj_pos,
            proj_motion,
            delta,
            player_pos,
            face_dir.0,
            gameplay_config.player.physics(),
        ) {
            closest_hit = Some(closer_hit(
                closest_hit,
                ProjectileTargetHit::Player { is_local_player, hit },
            ));
        }
    }

    for (actor_id, actor_pos, face_dir) in actor_query.iter() {
        let Some(info) = actors.0.get(actor_id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is in gameplay config")
            .physics();
        if let Some(hit) = projectile_character_hit(proj_pos, proj_motion, delta, actor_pos, face_dir.0, actor_physics)
        {
            closest_hit = Some(closer_hit(closest_hit, ProjectileTargetHit::Actor { hit }));
        }
    }

    match closest_hit {
        Some(ProjectileTargetHit::Player { is_local_player, .. }) => {
            play_sound(
                commands,
                asset_server,
                asset_set.player_sound("projectile_hits_player"),
                PlaybackSettings::DESPAWN,
            );

            if is_local_player {
                play_sound(
                    commands,
                    asset_server,
                    asset_set.player_sound("take_hit"),
                    PlaybackSettings::DESPAWN,
                );
            }

            commands.entity(proj_entity).despawn();
            true
        }
        Some(ProjectileTargetHit::Actor { .. }) => {
            commands.entity(proj_entity).despawn();
            true
        }
        None => false,
    }
}

fn play_sound(commands: &mut Commands, asset_server: &AssetServer, asset_path: &str, settings: PlaybackSettings) {
    commands.spawn((AudioPlayer::new(asset_server.load(asset_path.to_owned())), settings));
}

// ============================================================================
// Projectiles Movement System
// ============================================================================

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    mut projectile_query: Query<(Entity, &mut Transform, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    player_query: Query<(Entity, &Position, &FaceDirection, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position, &FaceDirection), With<ActorMarker>>,
    actors: Res<ActorMap>,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
    mut last_bounce_sound: ResMut<LastBounceSoundTime>,
) {
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let collision_world = collision_world.as_deref();

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

        // Resolve static world collisions and handle bouncing/despawning.
        let new_pos = if let Some(pos_after_bounce) = handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            &asset_set,
            &mut projectile,
            &projectile_pos,
            delta,
            collision_world,
            current_time,
            &mut last_bounce_sound,
        ) {
            pos_after_bounce
        } else {
            // Check character collisions
            if handle_character_collisions(
                &mut commands,
                asset_server.as_ref(),
                &asset_set,
                projectile_entity,
                &projectile,
                &projectile_pos,
                delta,
                *shooter_id,
                &player_query,
                &actor_query,
                &actors,
                &gameplay_config,
            ) {
                // Hit a character, projectile was despawned
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
    asset_set: &AssetSet,
    proj_motion: &mut ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSoundTime,
) -> Option<Position> {
    let collision_world = collision_world?;

    // Check speed before bounce to decide if we should play sound
    let speed_before = proj_motion.velocity.length();

    let new_pos = proj_motion.resolve_world_bounces(proj_pos, delta, collision_world)?;

    // Play sound if speed is high enough and rate limit allows
    let min_interval = 1.0 / PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND;
    if speed_before >= PROJECTILE_MIN_BOUNCE_SOUND_SPEED && current_time - last_bounce_sound.0 >= min_interval {
        play_sound(
            commands,
            asset_server,
            asset_set.player_sound("projectile_hits_wall"),
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

#[derive(Clone, Copy)]
enum ProjectileTargetHit {
    Player {
        is_local_player: bool,
        hit: ProjectileCharacterHit,
    },
    Actor {
        hit: ProjectileCharacterHit,
    },
}

impl ProjectileTargetHit {
    const fn hit(self) -> ProjectileCharacterHit {
        match self {
            Self::Player { hit, .. } | Self::Actor { hit, .. } => hit,
        }
    }
}

fn closer_hit(current: Option<ProjectileTargetHit>, candidate: ProjectileTargetHit) -> ProjectileTargetHit {
    match current {
        Some(current) if current.hit().time_of_impact <= candidate.hit().time_of_impact => current,
        _ => candidate,
    }
}
