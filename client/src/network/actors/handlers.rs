use bevy::{audio::SpatialScale, prelude::*};

use super::{super::context::ServerMessageContext, sync::apply_actor_movement_state};
use crate::{
    audio::{play_explosion_sound, play_spatial_sound},
    vfx::{spawn_actor_explosion, spawn_laser_beam},
};
use common::protocol::*;

pub(in crate::network) fn handle_actor_move_message(
    message: SActorMove,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    apply_actor_movement_state(
        commands,
        &context.actors,
        &context.rtt,
        &context.actor_data,
        message.id,
        message.movement,
        message.movement.move_intent.direction(),
    );
}

// Drives the immediate death of an actor on this client: explosion VFX +
// sound, then despawn the entity and drop the `ActorMap` entry. The
// snapshot diff is the fallback if this event was dropped.
//
// Kind isn't on the wire `SActorDeath`; we recover it from the local
// `ActorMap` where the kind was recorded when this actor was first seen.
pub(in crate::network) fn handle_actor_death_message(
    message: SActorDeath,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    // Early-apply the killer's post-bonus score so the HUD bumps on the kill
    // tick. Snapshot still authoritative.
    if let (Some(killer_id), Some(killer_score)) = (message.killer, message.killer_score)
        && let Some(killer_info) = context.players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    let Some(info) = context.actors.remove(&message.id) else {
        // Already torn down (e.g. via the snapshot diff). Stay idempotent.
        return;
    };
    spawn_actor_explosion(commands, &mut context.explosion_ctx(), &info.kind, message.pos);
    play_explosion_sound(
        commands,
        &context.asset_server,
        context.asset_set.actor_sound(&info.kind, "explodes"),
        &context.client_settings.audio,
        Vec3::from(message.pos),
        context.blast_radii.actors.get(&info.kind).copied(),
    );
    commands.entity(info.entity).despawn();
}

pub(in crate::network) fn handle_actor_hit_message(
    message: SActorHit,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    trace!("{:?} was hit", message.id);
    // Early-apply the post-hit health so the floating health bar drops on
    // the impact tick instead of waiting for the next snapshot.
    if let Some(info) = context.actors.get(&message.id) {
        commands.entity(info.entity).insert(message.health);
        // `SActorHit` is broadcast to every client, so the impact plays as
        // a world sound at the actor — distant fights plink faintly instead
        // of clicking at full volume map-wide.
        if let Ok((pos, _, _)) = context.actor_data.get(info.entity) {
            play_spatial_sound(
                commands,
                &context.asset_server,
                context.asset_set.player_sound("hit_actor"),
                &context.client_settings.audio,
                Vec3::from(*pos),
            );
        }
    }
}

// Spawn the beam visual for a burst, with the firing sound looping on the
// beam entity itself — the sound follows the beam and stops the moment the
// beam despawns (burst end, or either endpoint gone), so the continuous
// loop lasts exactly as long as the beam. An unknown actor id (cue raced
// its own snapshot teardown) spawns nothing — the beam would despawn on its
// first frame anyway.
pub(in crate::network) fn handle_actor_beam_message(
    message: SActorBeam,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let Some(info) = context.actors.get(&message.id) else {
        return;
    };
    let beam = spawn_laser_beam(commands, &mut context.meshes, &mut context.materials, &message);
    let mut beam_entity = commands.entity(beam);
    beam_entity.insert((
        AudioPlayer::new(
            context
                .asset_server
                .load(context.asset_set.actor_sound(&info.kind, "fire").to_owned()),
        ),
        PlaybackSettings::LOOP
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(context.client_settings.audio.spatial_distance_scale)),
    ));
    // Seed the transform at the actor so the loop's first frames are heard
    // from the right spot; the update system re-anchors it every frame.
    if let Ok((pos, _, _)) = context.actor_data.get(info.entity) {
        beam_entity.insert(Transform::from_translation(Vec3::from(*pos)));
    }
}
