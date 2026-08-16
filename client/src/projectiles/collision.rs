use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileCharacterHit, ProjectileMotion, projectile_character_hit},
    protocol::{ActorId, ActorMarker, BarrierKindId, FaceDirection, PlayerId, PlayerMarker, Position},
};

use super::audio::{
    LastBounceSound, play_barrier_impact_sound, play_sound, play_spatial_sound, play_wall_bounce_sound,
};
use crate::{
    actors::ActorMap,
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    players::LocalPlayerMarker,
    vfx::{ImpactKind, ParticleCloud, spawn_impact_sparks},
};

pub(super) fn handle_character_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    sparks: &mut ParticleCloud,
    settings: &ClientSettings,
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
        // Self-hits only count once the projectile has left the shooter's
        // hitbox (see `ProjectileMotion::left_shooter`).
        if shooter_id == *player_id && !proj_motion.left_shooter {
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
        let Some(info) = actors.get(actor_id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is missing from gameplay config")
            .physics();
        if let Some(hit) = projectile_character_hit(proj_pos, proj_motion, delta, actor_pos, face_dir.0, actor_physics)
        {
            closest_hit = Some(closer_hit(closest_hit, ProjectileTargetHit::Actor { hit }));
        }
    }

    match closest_hit {
        Some(target_hit) => {
            let hit = target_hit.hit();
            let impact = Vec3::from(*proj_pos) + proj_motion.velocity * delta * hit.time_of_impact;
            if let ProjectileTargetHit::Player { is_local_player, .. } = target_hit {
                // World sound at the impact — every client simulates every
                // projectile, so someone else's hit lands as a distant thud.
                play_spatial_sound(
                    commands,
                    asset_server,
                    asset_set.player_sound("hit_player"),
                    &settings.audio,
                    PlaybackSettings::DESPAWN,
                    impact,
                );

                // Personal cue: you got hit. Stays full-volume flat.
                if is_local_player {
                    play_sound(
                        commands,
                        asset_server,
                        asset_set.player_sound("take_hit"),
                        PlaybackSettings::DESPAWN,
                    );
                }
            }

            let outward = -proj_motion.velocity.normalize_or_zero();
            spawn_impact_sparks(
                sparks,
                &settings.vfx.projectiles.impact_sparks,
                impact,
                outward,
                outward,
                proj_motion.velocity.length(),
                ImpactKind::Character,
            );
            commands.entity(proj_entity).despawn();
            true
        }
        None => false,
    }
}

// Barriers terminate the projectile (no bounce). Returns `true` if the
// projectile hit a barrier this frame — caller despawns and skips the rest of
// the per-projectile pipeline.
pub(super) fn handle_barrier_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    sparks: &mut ParticleCloud,
    settings: &ClientSettings,
    barrier_assets: &BarrierAssets,
    proj_entity: Entity,
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    open_kinds: &[BarrierKindId],
) -> bool {
    let Some(collision_world) = collision_world else {
        return false;
    };
    let Some(impact) = proj_motion.terminate_at_barrier(proj_pos, delta, collision_world, open_kinds) else {
        return false;
    };
    play_barrier_impact_sound(commands, asset_server, asset_set, &settings.audio, impact.point);
    spawn_impact_sparks(
        sparks,
        &settings.vfx.projectiles.impact_sparks,
        impact.point,
        impact.normal,
        impact.normal,
        proj_motion.velocity.length(),
        ImpactKind::Barrier(barrier_assets.base_color(impact.kind)),
    );
    commands.entity(proj_entity).despawn();
    true
}

pub(super) fn handle_wall_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    sparks: &mut ParticleCloud,
    settings: &ClientSettings,
    proj_motion: &mut ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSound,
    listener_pos: Vec3,
) -> Option<Position> {
    let collision_world = collision_world?;

    let speed_before = proj_motion.velocity.length();
    let bounces = proj_motion.resolve_world_bounces(proj_pos, delta, collision_world)?;
    play_wall_bounce_sound(
        commands,
        asset_server,
        asset_set,
        &settings.audio,
        speed_before,
        current_time,
        last_bounce_sound,
        bounces.first_contact,
        listener_pos,
    );
    // Same-tick bounces each retain their local impact cue even though audio
    // is rate-limited globally. The shared particle budget bounds the burst.
    if speed_before >= settings.audio.projectile_impacts.min_bounce_speed_meters_per_second {
        spawn_impact_sparks(
            sparks,
            &settings.vfx.projectiles.impact_sparks,
            bounces.first_contact,
            bounces.first_normal,
            proj_motion.velocity.normalize_or_zero(),
            speed_before,
            ImpactKind::World,
        );
    }

    Some(bounces.position)
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
