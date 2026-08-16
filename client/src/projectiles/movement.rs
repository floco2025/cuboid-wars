use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, OpenBarrierKinds, ProjectileMotion, projectile_overlaps_character},
    protocol::{ActorId, ActorMarker, FaceDirection, PlayerId, PlayerMarker, Position, ProjectileMarker},
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
    player_query: Query<(Entity, &Position, &FaceDirection, &PlayerId, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position, &FaceDirection), With<ActorMarker>>,
    actors: Res<ActorMap>,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    mut last_bounce_sound: ResMut<LastBounceSound>,
    client_settings: Res<ClientSettings>,
    barrier_assets: Res<BarrierAssets>,
    mut particle_clouds: ResMut<ParticleClouds>,
    listener: Query<&GlobalTransform, With<MainCameraMarker>>,
) {
    let delta = time.delta_secs();
    let current_time = time.elapsed_secs();
    let collision_world = collision_world.as_deref();
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
        projectile.apply_gravity(delta);
        projectile.apply_drag(delta);

        let projectile_pos: Position = *position;

        // Arm self-hits once the projectile no longer overlaps the shooter
        // (mirrors the server). A missing shooter arms immediately.
        if !projectile.left_shooter {
            let overlaps_shooter = player_query
                .iter()
                .find(|(_, _, _, player_id, _)| *player_id == shooter_id)
                .is_some_and(|(_, player_pos, face_dir, _, _)| {
                    projectile_overlaps_character(
                        &projectile_pos,
                        player_pos,
                        face_dir.0,
                        gameplay_config.player.physics(),
                    )
                });
            if !overlaps_shooter {
                projectile.left_shooter = true;
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
            &open_barrier_kinds.0,
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
                &gameplay_config,
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
