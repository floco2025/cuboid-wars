use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    constants::{PHYSICS_EPSILON, PROJECTILE_EVENT_LIMIT},
    physics::{
        CollisionWorld, PortalSet, ProjectileEvent, ProjectileMotion, earliest_projectile_event,
        projectile_overlaps_character,
    },
    protocol::{
        ActorId, ActorMarker, FaceYaw, MapSettings, PlateState, PlayerId, PlayerMarker, Position, ProjectileMarker,
    },
};

use super::{
    audio::LastBounceSound,
    collision::{closest_character_hit, handle_barrier_collisions, present_character_impact, present_world_bounce},
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
    collision_world: Res<'w, CollisionWorld>,
    map_settings: Res<'w, MapSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    plates: Res<'w, PlateState>,
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
    let gravity = world.map_settings.movement.gravity * world.gameplay_config.projectiles.gravity_scale;
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let collision_world = &world.collision_world;
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

        let mut current_pos = *position;
        let mut remaining_delta = delta;
        let mut terminated = false;

        for _ in 0..PROJECTILE_EVENT_LIMIT {
            if remaining_delta <= PHYSICS_EPSILON {
                break;
            }

            if !projectile.left_shooter {
                let overlaps_shooter = player_query
                    .iter()
                    .find(|(_, _, _, player_id, _)| *player_id == shooter_id)
                    .is_some_and(|(_, player_pos, face_yaw, _, _)| {
                        projectile_overlaps_character(
                            &projectile,
                            &current_pos,
                            player_pos,
                            face_yaw.0,
                            world.gameplay_config.player.physics(),
                        )
                    });
                if !overlaps_shooter {
                    projectile.left_shooter = true;
                }
            }

            let character_hit = closest_character_hit(
                &projectile,
                &current_pos,
                remaining_delta,
                *shooter_id,
                &player_query,
                &actor_query,
                &actors,
                &world.gameplay_config,
            );
            let barrier_t = projectile.barrier_collision_t(
                &current_pos,
                remaining_delta,
                collision_world,
                &world.plates.open_barrier_kinds,
            );
            let surface_t = projectile.surface_collision_t(
                &current_pos,
                remaining_delta,
                collision_world,
                &world.plates.powered_bridge_kinds,
            );
            let portal_hop = world.portal_set.projectile_hop(
                Vec3::from(current_pos),
                projectile.velocity,
                remaining_delta,
                world.gameplay_config.projectiles.radius,
            );

            match earliest_projectile_event(
                character_hit.map(|hit| hit.hit().time_of_impact),
                barrier_t,
                surface_t,
                portal_hop.map(|hop| hop.t),
            ) {
                ProjectileEvent::Barrier => {
                    let hit = handle_barrier_collisions(
                        &mut commands,
                        asset_server.as_ref(),
                        &asset_set,
                        &mut particle_clouds.sparks,
                        &client_settings,
                        &barrier_assets,
                        projectile_entity,
                        &projectile,
                        &current_pos,
                        remaining_delta,
                        collision_world,
                        &world.plates.open_barrier_kinds,
                    );
                    assert!(hit, "barrier event missing its collision");
                    terminated = true;
                    break;
                }
                ProjectileEvent::Surface => {
                    let speed_before = projectile.velocity.length();
                    let bounce = projectile
                        .bounce_at_world_surface(
                            &current_pos,
                            remaining_delta,
                            collision_world,
                            &world.plates.powered_bridge_kinds,
                        )
                        .expect("surface event missing its collision");
                    current_pos = bounce.position;
                    remaining_delta = bounce.remaining_delta;
                    present_world_bounce(
                        &mut commands,
                        asset_server.as_ref(),
                        &asset_set,
                        &mut particle_clouds.sparks,
                        &client_settings,
                        &projectile,
                        bounce,
                        speed_before,
                        current_time,
                        &mut last_bounce_sound,
                        listener_pos,
                    );
                }
                ProjectileEvent::Portal => {
                    let hop = portal_hop.expect("portal event missing its crossing");
                    projectile.velocity = hop.exit_velocity;
                    current_pos = hop.exit_pos.into();
                    remaining_delta *= 1.0 - hop.t;
                    previous_tick_position.0 = current_pos;
                }
                ProjectileEvent::Hit => {
                    present_character_impact(
                        &mut commands,
                        asset_server.as_ref(),
                        &asset_set,
                        &mut particle_clouds.sparks,
                        &client_settings,
                        projectile_entity,
                        &projectile,
                        &current_pos,
                        remaining_delta,
                        character_hit.expect("character event missing its hit"),
                    );
                    terminated = true;
                    break;
                }
                ProjectileEvent::Fly => {
                    current_pos = (Vec3::from(current_pos) + projectile.velocity * remaining_delta).into();
                    break;
                }
            }
        }

        if !terminated {
            *position = current_pos;
        }
    }
}
