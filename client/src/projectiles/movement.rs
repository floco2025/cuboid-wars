use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, PortalSet},
    protocol::{
        ActorId, ActorMarker, CProjectileHit, ClientMessage, FaceYaw, MapSettings, PlayerId, PlayerMarker, Position,
        SwitchState,
    },
};

use super::{
    ProjectileEnvironment, ProjectileFlightEvent, ProjectileMarker, ProjectileMotion,
    audio::LastBounceSound,
    collision::{closest_character_hit, present_character_impact, present_field_impact, present_world_bounce},
    projectile_overlaps_character,
    spawn::EmberMarker,
    step_projectile,
};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    fields::FieldAssets,
    network::ClientToServerChannel,
    players::{LocalPlayerMarker, MyPlayerId, PlayerMap},
    vfx::ParticleClouds,
};

// The world a projectile flies through: collision, map gravity, tuning.
// Grouped so the system stays under Bevy's parameter limit.
#[derive(SystemParam)]
pub struct ProjectileWorld<'w> {
    collision_world: Res<'w, CollisionWorld>,
    map_settings: Res<'w, MapSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    switch_state: Res<'w, SwitchState>,
    portal_set: Res<'w, PortalSet>,
    players: Res<'w, PlayerMap>,
    my_player_id: Res<'w, MyPlayerId>,
    to_server: Res<'w, ClientToServerChannel>,
}
// Fixed steps keep gravity, drag, and ricochets independent of rendering frame rate.
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
            Has<EmberMarker>,
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
    field_assets: Res<FieldAssets>,
    mut particle_clouds: ResMut<ParticleClouds>,
    listener: Query<&GlobalTransform, With<MainCameraMarker>>,
) {
    let gravity = world.map_settings.movement.gravity * world.gameplay_config.projectiles.gravity_scale;
    let current_time = time.elapsed_secs();
    let collision_world = &world.collision_world;
    // Louder-bounce preference measures distance to the audio listener (the
    // main camera). A missing camera degrades to distance zero: every bounce
    // rates as full volume, which reduces to the plain rate limit.
    let listener_pos = listener
        .single()
        .map(|transform| transform.translation())
        .unwrap_or(Vec3::ZERO);

    for (projectile_entity, mut position, mut previous_tick_position, mut projectile, shooter_id, ember) in
        &mut projectile_query
    {
        let result = step_projectile(
            *position,
            &mut projectile,
            &ProjectileEnvironment {
                delta: time.delta(),
                gravity,
                collision_world,
                portals: &world.portal_set,
                open_fields: &world.switch_state.open_fields,
            },
            |projectile, current_pos| {
                player_query
                    .iter()
                    .find(|(entity, _, _, player_id, _)| {
                        *player_id == shooter_id
                            && world.players.get(player_id).is_some_and(|info| {
                                info.entity == *entity && world.players.accepts_generation(**player_id, info.generation)
                            })
                    })
                    .is_some_and(|(_, player_pos, face_yaw, _, _)| {
                        projectile_overlaps_character(
                            projectile,
                            current_pos,
                            player_pos,
                            face_yaw.0,
                            world.gameplay_config.player.physics(),
                        )
                    })
            },
            |projectile, current_pos, remaining| {
                closest_character_hit(
                    projectile,
                    current_pos,
                    remaining,
                    *shooter_id,
                    &player_query,
                    &actor_query,
                    &actors,
                    &world.players,
                    &world.gameplay_config,
                )
                .map(|hit| (hit, hit.hit()))
            },
        );
        for event in result.events {
            match event {
                ProjectileFlightEvent::Field { impact, speed } => present_field_impact(
                    &mut commands,
                    asset_server.as_ref(),
                    &asset_set,
                    &mut particle_clouds.sparks,
                    &client_settings,
                    &field_assets,
                    impact,
                    speed,
                ),
                ProjectileFlightEvent::Bounce {
                    bounce,
                    speed_before,
                    velocity,
                } => present_world_bounce(
                    &mut commands,
                    asset_server.as_ref(),
                    &asset_set,
                    &mut particle_clouds.sparks,
                    &client_settings,
                    velocity,
                    bounce,
                    speed_before,
                    current_time,
                    &mut last_bounce_sound,
                    listener_pos,
                ),
                ProjectileFlightEvent::Hit {
                    target,
                    hit,
                    position,
                    velocity,
                } => {
                    if !ember && *shooter_id == world.my_player_id.0 {
                        world.to_server.send(ClientMessage::ProjectileHit(CProjectileHit {
                            target: target.target(),
                            direction: [hit.direction.x, hit.direction.z],
                        }));
                    }
                    present_character_impact(
                        &mut commands,
                        asset_server.as_ref(),
                        &asset_set,
                        &mut particle_clouds.sparks,
                        &client_settings,
                        position,
                        velocity,
                        target,
                    );
                }
                ProjectileFlightEvent::Expired | ProjectileFlightEvent::Portal { .. } => {}
            }
        }
        if result.terminated {
            commands.entity(projectile_entity).despawn();
        } else {
            *position = result.position;
            previous_tick_position.0 = result.previous_position;
        }
    }
}
