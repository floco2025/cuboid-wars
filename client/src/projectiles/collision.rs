use crate::constants::PROJECTILE_IMPACT_MIN_BOUNCE_SPEED;
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{BallCharacterHit, CollisionWorld, ProjectileMotion, SurfaceBounce, projectile_character_hit},
    protocol::{ActorId, ActorMarker, BarrierKindId, FaceYaw, PlayerId, PlayerMarker, Position},
};

use super::audio::{
    LastBounceSound, play_barrier_impact_sound, play_sound_with, play_spatial_sound_with, play_wall_bounce_sound,
};
use crate::{
    actors::ActorMap,
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    players::LocalPlayerMarker,
    vfx::{ImpactKind, ParticleCloud, spawn_impact_sparks},
};

pub(super) fn closest_character_hit(
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    shooter_id: PlayerId,
    player_query: &Query<(Entity, &Position, &FaceYaw, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: &Query<(&ActorId, &Position, &FaceYaw), With<ActorMarker>>,
    actors: &ActorMap,
    gameplay_config: &GameplayConfig,
) -> Option<ProjectileTargetHit> {
    let mut closest_hit = None;

    for (_player_entity, player_pos, face_yaw, player_id, is_local_player) in player_query.iter() {
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
            face_yaw.0,
            gameplay_config.player.physics(),
        ) {
            closest_hit = Some(closer_hit(
                closest_hit,
                ProjectileTargetHit::Player { is_local_player, hit },
            ));
        }
    }

    for (actor_id, actor_pos, face_yaw) in actor_query.iter() {
        let Some(info) = actors.get(actor_id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is missing from gameplay config")
            .physics();
        if let Some(hit) = projectile_character_hit(proj_pos, proj_motion, delta, actor_pos, face_yaw.0, actor_physics)
        {
            closest_hit = Some(closer_hit(closest_hit, ProjectileTargetHit::Actor { hit }));
        }
    }

    closest_hit
}

pub(super) fn present_character_impact(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    sparks: &mut ParticleCloud,
    settings: &ClientSettings,
    proj_entity: Entity,
    proj_motion: &ProjectileMotion,
    proj_pos: &Position,
    delta: f32,
    target_hit: ProjectileTargetHit,
) {
    let hit = target_hit.hit();
    let impact = Vec3::from(*proj_pos) + proj_motion.velocity * delta * hit.time_of_impact;
    if let ProjectileTargetHit::Player { is_local_player, .. } = target_hit {
        play_spatial_sound_with(
            commands,
            asset_server,
            asset_set.player_sound("hit_player"),
            &settings.audio,
            PlaybackSettings::DESPAWN,
            impact,
        );
        if is_local_player {
            play_sound_with(
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
        impact,
        outward,
        outward,
        proj_motion.velocity.length(),
        ImpactKind::Character,
    );
    commands.entity(proj_entity).despawn();
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
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
) -> bool {
    let Some(impact) = proj_motion.terminate_at_barrier(proj_pos, delta, collision_world, open_kinds) else {
        return false;
    };
    play_barrier_impact_sound(commands, asset_server, asset_set, &settings.audio, impact.point);
    spawn_impact_sparks(
        sparks,
        impact.point,
        impact.normal,
        impact.normal,
        proj_motion.velocity.length(),
        ImpactKind::Barrier(barrier_assets.base_color(impact.kind)),
    );
    commands.entity(proj_entity).despawn();
    true
}

pub(super) fn present_world_bounce(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    sparks: &mut ParticleCloud,
    settings: &ClientSettings,
    proj_motion: &ProjectileMotion,
    bounce: SurfaceBounce,
    speed_before: f32,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSound,
    listener_pos: Vec3,
) {
    play_wall_bounce_sound(
        commands,
        asset_server,
        asset_set,
        &settings.audio,
        speed_before,
        current_time,
        last_bounce_sound,
        bounce.contact,
        listener_pos,
    );
    // Same-tick bounces each retain their local impact cue even though audio
    // is rate-limited globally. The shared particle budget bounds the burst.
    if speed_before >= PROJECTILE_IMPACT_MIN_BOUNCE_SPEED {
        spawn_impact_sparks(
            sparks,
            bounce.contact,
            bounce.normal,
            proj_motion.velocity.normalize_or_zero(),
            speed_before,
            ImpactKind::World,
        );
    }
}

#[derive(Clone, Copy)]
pub(super) enum ProjectileTargetHit {
    Player {
        is_local_player: bool,
        hit: BallCharacterHit,
    },
    Actor {
        hit: BallCharacterHit,
    },
}

impl ProjectileTargetHit {
    pub(super) const fn hit(self) -> BallCharacterHit {
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
