use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::ActorMap,
    combat::{
        DeathSource, PendingExplosions, apply_actor_projectile_hit, apply_player_projectile_hit, award_actor_kill,
        kill_player,
    },
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    network::broadcast_to_all,
    players::{Invincibility, PlayerMap},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    config::GameplayConfig,
    constants::{PHYSICS_EPSILON, PROJECTILE_EVENT_LIMIT},
    physics::{
        BallCharacterHit, CollisionWorld, PortalSet, ProjectileEvent, ProjectileMotion, earliest_projectile_event,
        projectile_character_hit, projectile_overlaps_character,
    },
    protocol::*,
};

#[derive(Clone, Copy)]
enum ProjectileTargetHit {
    Player { id: PlayerId, hit: BallCharacterHit },
    Actor { id: ActorId, hit: BallCharacterHit },
}

impl ProjectileTargetHit {
    const fn hit(self) -> BallCharacterHit {
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
        &'static FaceYaw,
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
        &'static FaceYaw,
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
    map_settings: Res<'w, MapSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
    quest_board: ResMut<'w, QuestBoard>,
    quest_catalog: Res<'w, QuestCatalog>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    actors: ResMut<'w, ActorMap>,
    players: ResMut<'w, PlayerMap>,
    pending_explosions: ResMut<'w, PendingExplosions>,
    invincibility: Res<'w, Invincibility>,
    portal_set: Res<'w, PortalSet>,
}

pub fn projectiles_movement_system(mut commands: Commands, time: Res<Time>, mut params: ProjectileMovementParams) {
    let delta = time.delta_secs();
    let gravity = params.map_settings.gravity * params.gameplay_config.projectiles.gravity_scale;

    for (proj_entity, mut proj_pos, mut projectile, shooter_id) in &mut params.projectile_query {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(proj_entity).despawn();
            continue;
        }

        projectile.apply_gravity(delta, gravity);
        projectile.apply_drag(delta);

        let mut current_pos = *proj_pos;
        let mut remaining_delta = delta;
        let mut closest_hit = None;
        let mut terminated = false;
        let mut event_budget_exhausted = true;

        for _ in 0..PROJECTILE_EVENT_LIMIT {
            if remaining_delta <= PHYSICS_EPSILON {
                closest_hit = None;
                event_budget_exhausted = false;
                break;
            }

            // A portal can move the projectile away from its shooter during
            // this tick, so arming is checked at the start of every segment.
            if !projectile.left_shooter {
                let overlaps_shooter = params
                    .players
                    .get(shooter_id)
                    .and_then(|info| info.entity())
                    .and_then(|entity| params.player_query.get(entity).ok())
                    .is_some_and(|(position, face_direction, _, _)| {
                        projectile_overlaps_character(
                            &projectile,
                            &current_pos,
                            position,
                            face_direction.0,
                            params.gameplay_config.player.physics(),
                        )
                    });
                if !overlaps_shooter {
                    projectile.left_shooter = true;
                }
            }

            closest_hit = None;
            for (position, face_direction, player_id, _) in &mut params.player_query {
                if shooter_id == player_id && !projectile.left_shooter {
                    continue;
                }
                if let Some(hit) = projectile_character_hit(
                    &current_pos,
                    &projectile,
                    remaining_delta,
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
                let actor_physics = params.gameplay_config.expect_actor(&info.spawn_kind).physics();
                if let Some(hit) = projectile_character_hit(
                    &current_pos,
                    &projectile,
                    remaining_delta,
                    position,
                    face_direction.0,
                    actor_physics,
                ) {
                    closest_hit = Some(closer_hit(
                        closest_hit,
                        ProjectileTargetHit::Actor { id: *actor_id, hit },
                    ));
                }
            }

            let character_t = closest_hit.map(|hit| hit.hit().time_of_impact);
            let barrier_t = projectile.barrier_collision_t(
                &current_pos,
                remaining_delta,
                &params.collision_world,
                &params.open_barrier_kinds.0,
            );
            let surface_t = projectile.surface_collision_t(&current_pos, remaining_delta, &params.collision_world);
            let portal_hop = params.portal_set.projectile_hop(
                Vec3::from(current_pos),
                projectile.velocity,
                remaining_delta,
                params.gameplay_config.projectiles.radius,
            );

            match earliest_projectile_event(character_t, barrier_t, surface_t, portal_hop.map(|hop| hop.t)) {
                ProjectileEvent::Barrier => {
                    commands.entity(proj_entity).despawn();
                    terminated = true;
                    event_budget_exhausted = false;
                    break;
                }
                ProjectileEvent::Surface => {
                    let bounce = projectile
                        .bounce_at_world_surface(&current_pos, remaining_delta, &params.collision_world)
                        .expect("surface event missing its collision");
                    current_pos = bounce.position;
                    remaining_delta = bounce.remaining_delta;
                }
                ProjectileEvent::Portal => {
                    let hop = portal_hop.expect("portal event missing its crossing");
                    projectile.velocity = hop.exit_velocity;
                    current_pos = hop.exit_pos.into();
                    remaining_delta *= 1.0 - hop.t;
                }
                ProjectileEvent::Hit => {
                    event_budget_exhausted = false;
                    break;
                }
                ProjectileEvent::Fly => {
                    current_pos = (Vec3::from(current_pos) + projectile.velocity * remaining_delta).into();
                    event_budget_exhausted = false;
                    break;
                }
            }
        }

        *proj_pos = current_pos;
        if terminated || event_budget_exhausted {
            continue;
        }

        match closest_hit {
            Some(ProjectileTargetHit::Player { id: player_id, hit }) => {
                if let Some(target_entity) = params.players.get(&player_id).and_then(|info| info.entity())
                    && let Ok((target_pos, _, _, mut health)) = params.player_query.get_mut(target_entity)
                {
                    let death_pos = *target_pos;
                    let was_lethal = apply_player_projectile_hit(
                        &mut params.players,
                        shooter_id,
                        player_id,
                        &mut health,
                        &params.server_gameplay_config,
                        params.invincibility.0,
                    );

                    broadcast_to_all(
                        &params.players,
                        ServerMessage::PlayerHit(SPlayerHit {
                            id: player_id,
                            kind: HitKind::Projectile,
                            hit_dir_x: hit.direction.x,
                            hit_dir_z: hit.direction.z,
                            health: *health,
                        }),
                    );

                    if was_lethal {
                        info!("{} died", params.players.describe(&player_id));
                        kill_player(
                            &mut commands,
                            &mut params.players,
                            player_id,
                            target_entity,
                            death_pos,
                            params.gameplay_config.player.respawn_secs,
                            DeathSource::Shot(*shooter_id),
                            &params.server_gameplay_config.feed,
                            &mut params.pending_explosions,
                        );
                    }
                }
                commands.entity(proj_entity).despawn();
            }
            Some(ProjectileTargetHit::Actor { id: actor_id, .. }) => {
                for (_, _, id, mut health) in &mut params.actor_query {
                    if *id != actor_id {
                        continue;
                    }

                    let spawn_kind = params
                        .actors
                        .get(id)
                        .expect("actor in query missing from ActorMap")
                        .spawn_kind
                        .clone();
                    let was_lethal = apply_actor_projectile_hit(
                        &mut params.players,
                        shooter_id,
                        &spawn_kind,
                        &mut health,
                        &params.server_gameplay_config,
                    );
                    if was_lethal {
                        award_actor_kill(
                            &mut params.players,
                            &mut params.quest_board,
                            &params.quest_catalog,
                            *shooter_id,
                            &spawn_kind,
                            &params.server_gameplay_config,
                        );
                        // Stash the killer so `actors_removal_system`'s
                        // `SActorDeath` broadcast can attribute the kill.
                        if let Some(info) = params.actors.get_mut(id) {
                            info.last_damager = Some(*shooter_id);
                        }
                    }
                    broadcast_to_all(
                        &params.players,
                        ServerMessage::ActorHit(SActorHit {
                            id: actor_id,
                            health: *health,
                        }),
                    );
                    break;
                }
                commands.entity(proj_entity).despawn();
            }
            None => {}
        }
    }
}
