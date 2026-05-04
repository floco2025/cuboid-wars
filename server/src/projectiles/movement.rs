use bevy::prelude::*;

use crate::{
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    resources::{ActorMap, PlayerMap},
};
use common::{
    config::GameplayConfig,
    health::apply_damage,
    physics::{CollisionWorld, ProjectileMarker, ProjectileMotion, projectile_character_hit},
    protocol::*,
};

use super::hits::{ProjectileTargetHit, closer_hit};

pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut ProjectileMotion, &PlayerId), With<ProjectileMarker>>,
    mut player_query: Query<
        (&Position, &FaceDirection, &PlayerId, &mut Health),
        (With<PlayerMarker>, Without<ActorMarker>, Without<ProjectileMarker>),
    >,
    mut actor_query: Query<
        (&Position, &FaceDirection, &ActorId, &mut Health),
        (With<ActorMarker>, Without<PlayerMarker>, Without<ProjectileMarker>),
    >,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    actors: Res<ActorMap>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut projectile_query {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        let mut bounced = false;
        if let Some(new_pos) = projectile.resolve_world_bounces(&proj_pos, delta, &collision_world) {
            *proj_pos = new_pos;
            bounced = true;
        }

        if bounced {
            continue;
        }

        let mut closest_hit = None;

        for (position, face_direction, player_id, _) in &mut player_query {
            if shooter_id == player_id {
                continue;
            }

            if let Some(hit) = projectile_character_hit(
                &proj_pos,
                &projectile,
                delta,
                position,
                face_direction.0,
                gameplay_config.player.physics(),
            ) {
                closest_hit = Some(closer_hit(
                    closest_hit,
                    ProjectileTargetHit::Player { id: *player_id, hit },
                ));
            }
        }

        for (position, face_direction, actor_id, _) in &mut actor_query {
            let info = actors.0.get(actor_id).expect("actor in query missing from ActorMap");
            let actor_physics = gameplay_config
                .actor(&info.spawn_kind)
                .expect("actor kind validated at startup")
                .physics();
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
                if let Some(target_entity) = players.0.get(&player_id).map(|info| info.entity)
                    && let Ok((_, _, _, mut health)) = player_query.get_mut(target_entity)
                {
                    info!("{:?} hits {:?}", shooter_id, player_id);
                    apply_damage(&mut health, server_gameplay_config.player.projectile_damage_to_player);

                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += 1;
                    }
                    if let Some(target_info) = players.0.get_mut(&player_id) {
                        target_info.hits -= 1;
                    }

                    broadcast_to_all(
                        &players,
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
                for (_, _, id, mut health) in &mut actor_query {
                    if *id != actor_id {
                        continue;
                    }

                    let info = actors.0.get(id).expect("actor in query missing from ActorMap");
                    let damage = server_gameplay_config
                        .actor(&info.spawn_kind)
                        .expect("actor kind validated at startup")
                        .projectile_damage_from_player;
                    info!("{:?} hits {:?}", shooter_id, actor_id);
                    apply_damage(&mut health, damage);
                    if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                        shooter_info.hits += 1;
                    }
                    broadcast_to_all(&players, ServerMessage::ActorHit(SActorHit { id: actor_id }));
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
