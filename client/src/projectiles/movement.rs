use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    constants::PORTAL_SURFACE_TIE_EPSILON,
    physics::{CollisionWorld, OpenBarrierKinds, PortalSet, ProjectileMotion, projectile_overlaps_character},
    protocol::{ActorId, ActorMarker, FaceYaw, MapSettings, PlayerId, PlayerMarker, Position, ProjectileMarker},
};

use super::{
    audio::LastBounceSound,
    collision::{handle_barrier_collisions, handle_character_collisions, handle_wall_collisions},
};
use crate::{
    actors::ActorMap,
    barriers::BarrierAssets,
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    players::LocalPlayerMarker,
    vfx::ParticleClouds,
};

// The world a projectile flies through: collision, map gravity, tuning.
// Grouped so the system stays under Bevy's parameter limit.
#[derive(SystemParam)]
pub struct ProjectileWorld<'w> {
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_settings: Option<Res<'w, MapSettings>>,
    gameplay_config: Res<'w, GameplayConfig>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    portal_set: Res<'w, PortalSet>,
}
// Runs in `FixedUpdate` at the shared `TICK_HZ`. The semi-implicit Euler
// integration in `ProjectileMotion` is step-size-dependent, so stepping at
// render rate would systematically diverge from the server's 30 Hz
// trajectories (and compound at every bounce).
pub fn projectiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    mut projectile_query: Query<
        (
            Entity,
            &mut Position,
            &mut PreviousTickPosition,
            &mut ProjectileMotion,
            &PlayerId,
        ),
        // The `Without`s make this provably disjoint from the player/actor
        // `&Position` queries below (B0001).
        (With<ProjectileMarker>, Without<PlayerMarker>, Without<ActorMarker>),
    >,
    player_query: Query<(Entity, &Position, &FaceYaw, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position, &FaceYaw), With<ActorMarker>>,
    actors: Res<ActorMap>,
    world: ProjectileWorld,
    mut last_bounce_sound: ResMut<LastBounceSound>,
    client_settings: Res<ClientSettings>,
    barrier_assets: Res<BarrierAssets>,
    mut particle_clouds: ResMut<ParticleClouds>,
    listener: Query<&GlobalTransform, With<MainCameraMarker>>,
) {
    // No map yet means no shots either; nothing to step.
    let Some(map_settings) = world.map_settings.as_deref() else {
        return;
    };
    let gravity = map_settings.gravity * world.gameplay_config.projectiles.gravity_scale;
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let collision_world = world.collision_world.as_deref();
    // Louder-bounce preference measures distance to the audio listener (the
    // main camera). A missing camera degrades to distance zero: every bounce
    // rates as full volume, which reduces to the plain rate limit.
    let listener_pos = listener
        .single()
        .map(|transform| transform.translation())
        .unwrap_or(Vec3::ZERO);

    for (projectile_entity, mut position, mut previous_tick_position, mut projectile, shooter_id) in
        &mut projectile_query
    {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        previous_tick_position.0 = *position;
        projectile.apply_gravity(delta, gravity);
        projectile.apply_drag(delta);

        let projectile_pos: Position = *position;

        // Arm self-hits once the projectile no longer overlaps the shooter
        // (mirrors the server). A missing shooter arms immediately.
        if !projectile.left_shooter {
            let overlaps_shooter = player_query
                .iter()
                .find(|(_, _, _, player_id, _)| *player_id == shooter_id)
                .is_some_and(|(_, player_pos, face_yaw, _, _)| {
                    projectile_overlaps_character(
                        &projectile,
                        &projectile_pos,
                        player_pos,
                        face_yaw.0,
                        world.gameplay_config.player.physics(),
                    )
                });
            if !overlaps_shooter {
                projectile.left_shooter = true;
            }
        }

        // Portal hop with the server's priority rule: the portal wins its
        // tie with the surface it sits on; a strictly earlier barrier still
        // shields it. Shared code + identical inputs keep this cosmetic sim
        // on the server's trajectory through portals.
        if let Some(collision_world) = collision_world
            && let Some(hop) = world.portal_set.projectile_hop(
                Vec3::from(projectile_pos),
                projectile.velocity,
                delta,
                world.gameplay_config.projectiles.radius,
            )
        {
            let barrier_t =
                projectile.barrier_collision_t(&projectile_pos, delta, collision_world, &world.open_barrier_kinds.0);
            let surface_t = projectile.surface_collision_t(&projectile_pos, delta, collision_world);
            if barrier_t.is_none_or(|bt| hop.t < bt)
                && surface_t.is_none_or(|st| hop.t <= st + PORTAL_SURFACE_TIE_EPSILON)
            {
                projectile.velocity = hop.exit_velocity;
                let translation = projectile.velocity * (delta * (1.0 - hop.t));
                let clamped = match collision_world.cast_moving_ball(
                    hop.exit_pos,
                    translation,
                    world.gameplay_config.projectiles.radius,
                ) {
                    Some(hit) => translation * hit.t,
                    None => translation,
                };
                // Anchor interpolation at the exit so the render pops there
                // instead of smearing a streak through the wall.
                previous_tick_position.0 = hop.exit_pos.into();
                *position = (hop.exit_pos + clamped).into();
                continue;
            }
        }

        if handle_barrier_collisions(
            &mut commands,
            asset_server.as_ref(),
            &asset_set,
            &mut particle_clouds.sparks,
            &client_settings,
            &barrier_assets,
            projectile_entity,
            &projectile,
            &projectile_pos,
            delta,
            collision_world,
            &world.open_barrier_kinds.0,
        ) {
            continue;
        }

        let new_pos = if let Some(pos_after_bounce) = handle_wall_collisions(
            &mut commands,
            asset_server.as_ref(),
            &asset_set,
            &mut particle_clouds.sparks,
            &client_settings,
            &mut projectile,
            &projectile_pos,
            delta,
            collision_world,
            current_time,
            &mut last_bounce_sound,
            listener_pos,
        ) {
            pos_after_bounce
        } else {
            if handle_character_collisions(
                &mut commands,
                asset_server.as_ref(),
                &asset_set,
                &mut particle_clouds.sparks,
                &client_settings,
                projectile_entity,
                &projectile,
                &projectile_pos,
                delta,
                *shooter_id,
                &player_query,
                &actor_query,
                &actors,
                &world.gameplay_config,
            ) {
                continue;
            }

            Position {
                x: projectile.velocity.x.mul_add(delta, projectile_pos.x),
                y: projectile.velocity.y.mul_add(delta, projectile_pos.y),
                z: projectile.velocity.z.mul_add(delta, projectile_pos.z),
            }
        };

        *position = new_pos;
    }
}
