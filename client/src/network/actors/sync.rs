use bevy::prelude::*;
use std::collections::HashSet;

use super::super::{components::ServerReconciliation, context::ServerMessageContext};
use crate::{
    actors::{ActorInfo, ActorMap, beam_in_ghost_state, spawn_actor, spawn_actor_ghost},
    network::RoundTripTime,
};
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{Actor, ActorId, ActorMarker, ActorMoveIntent, ActorMovementState, FaceYaw, Position, SpawningActor},
};

pub(in crate::network) fn sync_actors(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    server_actors: &[(ActorId, Actor)],
) {
    let update_ids: HashSet<ActorId> = server_actors.iter().map(|(id, _)| *id).collect();

    for (id, actor) in server_actors {
        if context.actors.contains_key(id) {
            continue;
        }

        let entity = spawn_actor(
            commands,
            &context.asset_server,
            &mut context.meshes,
            &mut context.materials,
            &mut context.graphs,
            &context.asset_set,
            &context.client_settings,
            &context.gameplay_config,
            &context.max_health,
            *id,
            actor,
        );
        context.actors.insert(
            *id,
            ActorInfo {
                entity,
                kind: actor.kind.clone(),
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
        if let Some(client_actor) = context.actors.get(id) {
            commands.entity(client_actor.entity).insert(server_actor.health);
        }
        apply_actor_movement_state(
            commands,
            &context.actors,
            &context.rtt,
            &context.actor_data,
            *id,
            server_actor.movement,
            Some(server_actor.face_yaw),
        );
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
            let update = beam_in_ghost_state(&context.gameplay_config, spawning);
            commands
                .entity(entity)
                .entry::<crate::vfx::BeamInGhost>()
                .and_modify(move |mut ghost| ghost.resync(update))
                .or_insert(update);
            continue;
        }
        let entity = spawn_actor_ghost(
            commands,
            &context.asset_server,
            &context.asset_set,
            &context.gameplay_config,
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
    rtt: &RoundTripTime,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    id: ActorId,
    movement: ActorMovementState,
    face_yaw: Option<f32>,
) {
    let Some(client_actor) = actors.get(&id) else {
        return;
    };

    let server_velocity = actor_movement_velocity(movement);
    commands.entity(client_actor.entity).insert((
        movement.move_intent,
        CharacterVerticalVelocity(movement.vertical_velocity),
    ));
    if let Some(face_yaw) = face_yaw {
        commands.entity(client_actor.entity).insert(FaceYaw(face_yaw));
    }

    if let Ok((client_pos, _, _)) = actor_data.get(client_actor.entity) {
        commands.entity(client_actor.entity).insert(ServerReconciliation::new(
            *client_pos,
            movement.pos,
            server_velocity,
            rtt,
        ));
    }
}

fn actor_movement_velocity(movement: ActorMovementState) -> Vec3 {
    let mut velocity = movement.move_intent.to_horizontal_velocity();
    velocity.y = movement.vertical_velocity;
    velocity
}
