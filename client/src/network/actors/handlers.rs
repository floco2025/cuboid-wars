use bevy::{audio::SpatialScale, audio::Volume, prelude::*};

use super::sync::apply_actor_movement_state;
use crate::{
    actors::ActorMap,
    config::{AssetSet, AudioConfig},
    network::RoundTripTime,
    vfx::{ExplosionAssets, ExplosionRadii, ExplosionVfxBudget, explosion_sound_speed, spawn_actor_explosion},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{ActorMarker, ActorMoveIntent, FaceDirection, Position, SActorDeath, SActorHit, SActorMoveIntent},
};

pub fn handle_actor_move_intent_message(
    commands: &mut Commands,
    actors: &ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    msg: SActorMoveIntent,
) {
    apply_actor_movement_state(
        commands,
        actors,
        rtt,
        actor_data,
        msg.id,
        msg.movement,
        msg.movement.move_intent.direction(),
    );
}

// Drives the immediate death of an actor on this client: explosion VFX +
// sound, then despawn the entity and drop the `ActorMap` entry. The
// snapshot diff is the fallback if this event was dropped.
//
// Kind isn't on the wire `SActorDeath`; we recover it from the local
// `ActorMap` where the kind was recorded when this actor was first seen.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_actor_death_message(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    explosion_assets: &ExplosionAssets,
    actor_explosion_radii: &ExplosionRadii,
    actors: &mut ResMut<ActorMap>,
    players: &mut crate::players::PlayerMap,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    map_layout: Option<&common::protocol::MapLayout>,
    msg: SActorDeath,
) {
    // Early-apply the killer's post-bonus score so the HUD bumps on the kill
    // tick. Snapshot still authoritative.
    if let (Some(killer_id), Some(killer_score)) = (msg.killer, msg.killer_score)
        && let Some(killer_info) = players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    let Some(info) = actors.remove(&msg.id) else {
        // Already torn down (e.g. via the snapshot diff). Stay idempotent.
        return;
    };
    spawn_actor_explosion(
        commands,
        meshes,
        materials,
        budget,
        explosion_assets,
        actor_explosion_radii,
        gameplay_config,
        collision_world,
        map_layout,
        &info.kind,
        msg.pos,
    );
    // Spatial: attenuates and pans with distance from the blast. The scale
    // compresses world meters so the falloff suits map-sized distances.
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(&info.kind, "explodes").to_owned())),
        PlaybackSettings::DESPAWN
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale))
            .with_volume(Volume::Linear(audio_config.explosion_gain_multiplier))
            .with_speed(
                actor_explosion_radii
                    .actors
                    .get(&info.kind)
                    .copied()
                    .map_or(1.0, explosion_sound_speed),
            ),
        Transform::from_translation(Vec3::from(msg.pos)),
    ));
    commands.entity(info.entity).despawn();
}

pub fn handle_actor_hit_message(
    commands: &mut Commands,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    msg: SActorHit,
) {
    trace!("{:?} was hit", msg.id);
    // Early-apply the post-hit health so the floating health bar drops on
    // the impact tick instead of waiting for the next snapshot.
    if let Some(info) = actors.get(&msg.id) {
        commands.entity(info.entity).insert(msg.health);
        // `SActorHit` is broadcast to every client, so the impact plays as
        // a world sound at the actor — distant fights plink faintly instead
        // of clicking at full volume map-wide.
        if let Ok((pos, _, _)) = actor_data.get(info.entity) {
            commands.spawn((
                AudioPlayer::new(asset_server.load(asset_set.player_sound("hit_actor").to_owned())),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
                Transform::from_translation(Vec3::from(*pos)),
            ));
        }
    }
}
