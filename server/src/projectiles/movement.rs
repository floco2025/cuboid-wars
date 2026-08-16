use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::ActorMap,
    combat::{
        PendingExplosions, apply_actor_projectile_hit, apply_player_projectile_hit, award_actor_kill, kill_player,
    },
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    network::broadcast_to_all,
    players::{Invincibility, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{
        CollisionWorld, ProjectileCharacterHit, ProjectileMarker, ProjectileMotion, projectile_character_hit,
        projectile_overlaps_character,
    },
    protocol::*,
};

#[derive(Clone, Copy)]
enum ProjectileTargetHit {
    Player { id: PlayerId, hit: ProjectileCharacterHit },
    Actor { id: ActorId, hit: ProjectileCharacterHit },
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

#[derive(Debug, PartialEq, Eq)]
enum ProjectileEvent {
    Hit,
    Terminate,
    Bounce,
    Fly,
}

// Pick the earliest event along the projectile's straight path this tick, by
// time-of-impact in `[0, 1]`. World surfaces win ties against a character hit
// so a target behind cover is protected, and a barrier is checked before a
// bounce surface (preserving the barrier-terminates priority).
fn earliest_projectile_event(
    character_t: Option<f32>,
    barrier_t: Option<f32>,
    surface_t: Option<f32>,
) -> ProjectileEvent {
    if let Some(bt) = barrier_t
        && character_t.is_none_or(|ct| bt <= ct)
    {
        return ProjectileEvent::Terminate;
    }
    if let Some(st) = surface_t
        && character_t.is_none_or(|ct| st <= ct)
    {
        return ProjectileEvent::Bounce;
    }
    if character_t.is_some() {
        ProjectileEvent::Hit
    } else {
        ProjectileEvent::Fly
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
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    actors: ResMut<'w, ActorMap>,
    players: ResMut<'w, PlayerMap>,
    pending_explosions: ResMut<'w, PendingExplosions>,
    invincibility: Res<'w, Invincibility>,
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

        // Arm self-hits once the projectile no longer overlaps the shooter.
        // A missing shooter (died/logged off) arms immediately.
        if !projectile.left_shooter {
            let overlaps_shooter = params
                .players
                .get(shooter_id)
                .and_then(|info| params.player_query.get(info.entity).ok())
                .is_some_and(|(position, face_direction, _, _)| {
                    projectile_overlaps_character(
                        &proj_pos,
                        position,
                        face_direction.0,
                        params.gameplay_config.player.physics(),
                    )
                });
            if !overlaps_shooter {
                projectile.left_shooter = true;
            }
        }

        // Gather the closest player/actor hit BEFORE resolving world
        // collisions, so their times-of-impact can be compared: a target in
        // front of a wall must register a hit instead of being phased through
        // when a barrier/bounce would otherwise short-circuit the tick.
        let mut closest_hit = None;

        for (position, face_direction, player_id, _) in &mut params.player_query {
            if shooter_id == player_id && !projectile.left_shooter {
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

        // Resolve whichever event is earliest this tick.
        let character_t = closest_hit.map(|hit| hit.hit().time_of_impact);
        let barrier_t =
            projectile.barrier_collision_t(&proj_pos, delta, &params.collision_world, &params.open_barrier_kinds.0);
        let surface_t = projectile.surface_collision_t(&proj_pos, delta, &params.collision_world);

        match earliest_projectile_event(character_t, barrier_t, surface_t) {
            ProjectileEvent::Terminate => {
                commands.entity(proj_entity).despawn();
                continue;
            }
            ProjectileEvent::Bounce => {
                if let Some(bounces) = projectile.resolve_world_bounces(&proj_pos, delta, &params.collision_world) {
                    *proj_pos = bounces.position;
                }
                continue;
            }
            // Hit → the match below applies it; Fly → its `None` arm advances.
            ProjectileEvent::Hit | ProjectileEvent::Fly => {}
        }

        match closest_hit {
            Some(ProjectileTargetHit::Player { id: player_id, hit }) => {
                if let Some(target_entity) = params.players.get(&player_id).map(|info| info.entity)
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
                        info!("{:?} died", player_id);
                        // A self-kill is a solo death — no killer attribution
                        // (the feed would otherwise read "X killed X").
                        let killer = (player_id != *shooter_id).then_some(*shooter_id);
                        kill_player(
                            &mut commands,
                            &mut params.players,
                            player_id,
                            target_entity,
                            death_pos,
                            params.gameplay_config.player.respawn_delay_secs,
                            killer,
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
                        &mut health,
                        &spawn_kind,
                        &params.server_gameplay_config,
                    );
                    if was_lethal {
                        award_actor_kill(
                            &mut params.players,
                            *shooter_id,
                            &spawn_kind,
                            &params.server_gameplay_config,
                        );
                        // Stash the killer so `actor_removal_system`'s
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
            None => {
                *proj_pos += projectile.velocity * delta;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectileEvent, earliest_projectile_event};

    #[test]
    fn earliest_event_prefers_closest_with_world_winning_ties() {
        // Character strictly closest → the hit registers.
        assert_eq!(
            earliest_projectile_event(Some(0.2), Some(0.5), Some(0.6)),
            ProjectileEvent::Hit
        );
        // A closer barrier / bounce surface protects a target behind it.
        assert_eq!(
            earliest_projectile_event(Some(0.5), Some(0.3), None),
            ProjectileEvent::Terminate
        );
        assert_eq!(
            earliest_projectile_event(Some(0.5), None, Some(0.3)),
            ProjectileEvent::Bounce
        );
        // Ties go to the world surface (conservative cover).
        assert_eq!(
            earliest_projectile_event(Some(0.4), Some(0.4), None),
            ProjectileEvent::Terminate
        );
        assert_eq!(
            earliest_projectile_event(Some(0.4), None, Some(0.4)),
            ProjectileEvent::Bounce
        );
        // No world collision but a character hit → hit.
        assert_eq!(earliest_projectile_event(Some(0.4), None, None), ProjectileEvent::Hit);
        // No character: barrier keeps priority over the bounce surface.
        assert_eq!(
            earliest_projectile_event(None, Some(0.5), Some(0.3)),
            ProjectileEvent::Terminate
        );
        // Empty path → fly straight.
        assert_eq!(earliest_projectile_event(None, None, None), ProjectileEvent::Fly);
    }
}
