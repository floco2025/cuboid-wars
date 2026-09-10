use bevy::prelude::*;
use std::collections::HashSet;

use super::super::context::ServerMessageContext;
use crate::{
    actors::{ActorInfo, ActorMap, RemoteActorMotion, beam_in_ghost_state, spawn_actor, spawn_actor_ghost},
    network::SampleTiming,
};
use common::protocol::{Actor, ActorId, ActorMovementState, CarrierId, SpawningActor};

pub(in crate::network) fn sync_actors(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    tick: u32,
    server_actors: &[(ActorId, Actor)],
) {
    let update_ids: HashSet<ActorId> = server_actors.iter().map(|(id, _)| *id).collect();

    for (id, actor) in server_actors {
        if context.actors.contains_key(id) {
            continue;
        }

        let buffer = RemoteActorMotion::new(
            tick,
            actor.movement,
            SampleTiming::new(&context.client_settings.interpolation, &context.network),
        );
        let mut actor = actor.clone();
        actor.movement.pos = context
            .carriers
            .pose(actor.movement.carrier)
            .transform_position(&actor.movement.pos);
        actor.movement.carrier = CarrierId::WORLD;
        let entity = spawn_actor(
            commands,
            &context.assets.asset_server,
            &mut context.meshes,
            &mut context.materials,
            &context.assets.asset_set,
            &context.client_settings,
            &context.gameplay_config,
            &context.assets.max_health,
            *id,
            &actor,
        );
        commands.entity(entity).insert(buffer);
        context.actors.insert(
            *id,
            ActorInfo {
                entity,
                kind: actor.kind.clone(),
                beam: Default::default(),
            },
        );
    }

    context.actors.retain(|id, actor| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(actor.entity).despawn();
            false
        }
    });

    for (id, server_actor) in server_actors {
        if let Some(client_actor) = context.actors.get_mut(id) {
            client_actor.beam.apply(tick, server_actor.beam);
            commands.entity(client_actor.entity).insert(server_actor.health);
        }
        apply_actor_movement_state(commands, &context.actors, tick, *id, server_actor.movement);
    }
}

// Same diff idiom as `sync_actors`, over the snapshot's pending-spawn list:
// a new id grows a beam-in ghost, a vanished id tears it down. The real
// actor arrives in the same snapshot its ghost entry disappears from, so the
// handoff is seamless.
pub(in crate::network) fn sync_spawning_actors(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    spawning_actors: &[(ActorId, SpawningActor)],
) {
    let update_ids: HashSet<ActorId> = spawning_actors.iter().map(|(id, _)| *id).collect();

    for (id, spawning) in spawning_actors {
        if let Some(entity) = context.actor_ghosts.get(id) {
            commands
                .entity(entity)
                .insert(beam_in_ghost_state(&context.gameplay_config, spawning));
            continue;
        }
        let entity = spawn_actor_ghost(
            commands,
            &context.assets.asset_server,
            &context.assets.asset_set,
            &context.gameplay_config,
            context.carrier_entities.get(spawning.carrier),
            spawning,
        );
        context.actor_ghosts.insert(*id, entity);
    }

    context.actor_ghosts.retain(|id, entity| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}

pub(super) fn apply_actor_movement_state(
    commands: &mut Commands,
    actors: &ActorMap,
    tick: u32,
    id: ActorId,
    movement: ActorMovementState,
) {
    let Some(actor) = actors.get(&id) else {
        return;
    };
    commands.entity(actor.entity).queue(move |mut entity: EntityWorldMut| {
        if let Some(mut buffer) = entity.get_mut::<RemoteActorMotion>() {
            buffer.push(tick, movement);
        }
    });
}
