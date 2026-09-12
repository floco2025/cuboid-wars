use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::apply_actor_movement_state};
use crate::{
    audio::{play_explosion_sound, play_spatial_sound},
    vfx::spawn_actor_explosion,
};
use common::protocol::*;

pub(in crate::network) fn handle_actor_moves_message(
    message: SActorMoves,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    for entry in message.moves {
        apply_actor_movement_state(commands, &context.actors, message.tick, entry.id, entry.movement);
    }
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
        &context.assets.asset_server,
        context
            .assets
            .asset_set
            .actor_sound(&info.kind, "explodes")
            .expect("actor explosion sound missing"),
        &context.client_settings.audio,
        Vec3::from(message.pos),
        context.assets.blast_radii.actors.get(&info.kind).copied(),
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
        if let Ok(pos) = context.actor_data.get(info.entity) {
            play_spatial_sound(
                commands,
                &context.assets.asset_server,
                context.assets.asset_set.player_sound("hit_actor"),
                &context.client_settings.audio,
                Vec3::from(*pos),
            );
        }
    }
}

pub(in crate::network) fn handle_actor_beam_message(message: SActorBeam, context: &mut ServerMessageContext) {
    if let Some(actor) = context.actors.get_mut(&message.id) {
        actor.beam.apply(message.tick, message.beam);
    }
}
