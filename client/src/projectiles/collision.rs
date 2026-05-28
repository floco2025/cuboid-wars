use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileCharacterHit, ProjectileMotion, projectile_character_hit},
    protocol::{ActorId, ActorMarker, BarrierKindId, FaceDirection, PlayerId, PlayerMarker, Position},
};

use super::audio::{LastBounceSoundTime, play_barrier_impact_sound, play_sound, play_wall_bounce_sound};
use crate::{actors::ActorMap, config::AssetSet, players::LocalPlayerMarker};

pub(super) fn handle_character_collisions(
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
        let Some(info) = actors.get(actor_id) else {
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
                asset_set.player_sound("hit_player"),
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

// Barriers terminate the projectile (no bounce). Returns `true` if the
// projectile hit a barrier this frame — caller despawns and skips the rest of
// the per-projectile pipeline.
pub(super) fn handle_barrier_collisions(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
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
    if proj_motion
        .terminate_at_barrier(proj_pos, delta, collision_world, open_kinds)
        .is_none()
    {
        return false;
    }
    play_barrier_impact_sound(commands, asset_server, asset_set);
    commands.entity(proj_entity).despawn();
    true
}

pub(super) fn handle_wall_collisions(
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

    let speed_before = proj_motion.velocity.length();
    let new_pos = proj_motion.resolve_world_bounces(proj_pos, delta, collision_world)?;
    play_wall_bounce_sound(
        commands,
        asset_server,
        asset_set,
        speed_before,
        current_time,
        last_bounce_sound,
    );

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
