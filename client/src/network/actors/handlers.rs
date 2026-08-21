use bevy::{audio::SpatialScale, prelude::*};

use super::sync::apply_actor_movement_state;
use crate::{
    actors::ActorMap,
    audio::{play_explosion_sound, play_spatial_sound},
    config::{AssetSet, AudioConfig},
    network::RoundTripTime,
    vfx::{ExplosionRadii, ExplosionSpawnCtx, spawn_actor_explosion, spawn_laser_beam},
};
use common::protocol::{
    ActorMarker, ActorMoveIntent, FaceYaw, Position, SActorBeam, SActorDeath, SActorHit, SActorMove,
};

pub fn handle_actor_move_intent_message(
    commands: &mut Commands,
    actors: &ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    msg: SActorMove,
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
    ctx: &mut ExplosionSpawnCtx,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    actor_explosion_radii: &ExplosionRadii,
    actors: &mut ResMut<ActorMap>,
    players: &mut crate::players::PlayerMap,
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
    spawn_actor_explosion(commands, ctx, actor_explosion_radii, &info.kind, msg.pos);
    play_explosion_sound(
        commands,
        asset_server,
        asset_set.actor_sound(&info.kind, "explodes"),
        audio_config,
        Vec3::from(msg.pos),
        actor_explosion_radii.actors.get(&info.kind).copied(),
    );
    commands.entity(info.entity).despawn();
}

// Spawn the beam visual for a burst, with the firing sound looping on the
// beam entity itself — the sound follows the beam and stops the moment the
// beam despawns (burst end, or either endpoint gone), so the continuous
// loop lasts exactly as long as the beam. An unknown actor id (cue raced
// its own snapshot teardown) spawns nothing — the beam would despawn on its
// first frame anyway.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_actor_beam_message(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    msg: SActorBeam,
) {
    let Some(info) = actors.get(&msg.id) else {
        return;
    };
    let beam = spawn_laser_beam(commands, meshes, materials, &msg);
    let mut beam_entity = commands.entity(beam);
    beam_entity.insert((
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(&info.kind, "fire").to_owned())),
        PlaybackSettings::LOOP
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
    ));
    // Seed the transform at the actor so the loop's first frames are heard
    // from the right spot; the update system re-anchors it every frame.
    if let Ok((pos, _, _)) = actor_data.get(info.entity) {
        beam_entity.insert(Transform::from_translation(Vec3::from(*pos)));
    }
}

pub fn handle_actor_hit_message(
    commands: &mut Commands,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
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
            play_spatial_sound(
                commands,
                asset_server,
                asset_set.player_sound("hit_actor"),
                audio_config,
                Vec3::from(*pos),
            );
        }
    }
}
