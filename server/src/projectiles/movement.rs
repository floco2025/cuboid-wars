use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    combat::{apply_actor_projectile_hit, apply_player_projectile_hit},
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    resources::{ActorMap, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileMarker, ProjectileMotion, projectile_character_hit},
    protocol::*,
};

use super::hits::{ProjectileTargetHit, closer_hit};

type ProjectileQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static mut ProjectileMotion,
        &'static PlayerId,
    ),
    With<ProjectileMarker>,
>;

type ProjectilePlayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static FaceDirection,
        &'static PlayerId,
        &'static mut Health,
    ),
    (With<PlayerMarker>, Without<ActorMarker>, Without<ProjectileMarker>),
>;

type ProjectileActorQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static FaceDirection,
        &'static ActorId,
        &'static mut Health,
    ),
    (With<ActorMarker>, Without<PlayerMarker>, Without<ProjectileMarker>),
>;

#[derive(SystemParam)]
pub struct ProjectileMovementParams<'w, 's> {
    projectile_query: ProjectileQuery<'w, 's>,
    player_query: ProjectilePlayerQuery<'w, 's>,
    actor_query: ProjectileActorQuery<'w, 's>,
    collision_world: Res<'w, CollisionWorld>,
    gameplay_config: Res<'w, GameplayConfig>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
    actors: Res<'w, ActorMap>,
    players: ResMut<'w, PlayerMap>,
}

pub fn projectiles_movement_system(mut commands: Commands, time: Res<Time>, mut params: ProjectileMovementParams) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut params.projectile_query {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        let mut bounced = false;
        if let Some(new_pos) = projectile.resolve_world_bounces(&proj_pos, delta, &params.collision_world) {
            *proj_pos = new_pos;
            bounced = true;
        }

        if bounced {
            continue;
        }

        let mut closest_hit = None;

        for (position, face_direction, player_id, _) in &mut params.player_query {
            if shooter_id == player_id {
                continue;
            }

            if let Some(hit) = projectile_character_hit(
                &proj_pos,
                &projectile,
                delta,
                position,
                face_direction.0,
                params.gameplay_config.player.physics(),
            ) {
                closest_hit = Some(closer_hit(
                    closest_hit,
                    ProjectileTargetHit::Player { id: *player_id, hit },
                ));
            }
        }

        for (position, face_direction, actor_id, _) in &mut params.actor_query {
            let info = params
                .actors
                .get(actor_id)
                .expect("actor in query missing from ActorMap");
            let actor_physics = params.gameplay_config.validated_actor(&info.spawn_kind).physics();
            if let Some(hit) =
                projectile_character_hit(&proj_pos, &projectile, delta, position, face_direction.0, actor_physics)
            {
                closest_hit = Some(closer_hit(
                    closest_hit,
                    ProjectileTargetHit::Actor { id: *actor_id, hit },
                ));
            }
        }

        match closest_hit {
            Some(ProjectileTargetHit::Player { id: player_id, hit }) => {
                if let Some(target_entity) = params.players.get(&player_id).map(|info| info.entity)
                    && let Ok((_, _, _, mut health)) = params.player_query.get_mut(target_entity)
                {
                    info!("{:?} hits {:?}", shooter_id, player_id);
                    apply_player_projectile_hit(
                        &mut params.players,
                        shooter_id,
                        player_id,
                        &mut health,
                        &params.server_gameplay_config,
                    );

                    broadcast_to_all(
                        &params.players,
                        ServerMessage::Hit(SHit {
                            id: player_id,
                            hit_dir_x: hit.direction.x,
                            hit_dir_z: hit.direction.z,
                        }),
                    );
                }
                commands.entity(proj_entity).despawn();
            }
            Some(ProjectileTargetHit::Actor { id: actor_id, .. }) => {
                for (_, _, id, mut health) in &mut params.actor_query {
                    if *id != actor_id {
                        continue;
                    }

                    let info = params.actors.get(id).expect("actor in query missing from ActorMap");
                    info!("{:?} hits {:?}", shooter_id, actor_id);
                    apply_actor_projectile_hit(
                        &mut params.players,
                        shooter_id,
                        &mut health,
                        &info.spawn_kind,
                        &params.server_gameplay_config,
                    );
                    broadcast_to_all(&params.players, ServerMessage::ActorHit(SActorHit { id: actor_id }));
                    break;
                }
                commands.entity(proj_entity).despawn();
            }
            None => {
                *proj_pos += projectile.velocity * delta;
            }
        }
    }
}
