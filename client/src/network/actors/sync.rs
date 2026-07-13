use bevy::prelude::*;
use std::collections::HashSet;

use super::super::components::ServerReconciliation;
use crate::{
    actors::{ActorGhostMap, ActorInfo, ActorMap, beam_in_ghost_state, spawn_actor, spawn_actor_ghost},
    config::{AssetSet, ClientSettings},
    network::RoundTripTime,
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{
        Actor, ActorId, ActorMarker, ActorMoveIntent, ActorMovementState, FaceDirection, Position, SpawningActor,
    },
};

pub fn sync_actors(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    actors: &mut ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    gameplay_config: &GameplayConfig,
    server_actors: &[(ActorId, Actor)],
) {
    let update_ids: HashSet<ActorId> = server_actors.iter().map(|(id, _)| *id).collect();

    for (id, actor) in server_actors {
        if actors.contains_key(id) {
            continue;
        }

        let entity = spawn_actor(
            commands,
            asset_server,
            meshes,
            materials,
            graphs,
            asset_set,
            client_settings,
            gameplay_config,
            *id,
            actor,
        );
        actors.insert(
            *id,
            ActorInfo {
                entity,
                kind: actor.kind.clone(),
            },
        );
    }

    actors.retain(|id, actor| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(actor.entity).despawn();
            false
        }
    });

    for (id, server_actor) in server_actors {
        if let Some(client_actor) = actors.get(id) {
            commands.entity(client_actor.entity).insert(server_actor.health);
        }
        apply_actor_movement_state(
            commands,
            actors,
            rtt,
            actor_data,
            *id,
            server_actor.movement,
            Some(server_actor.face_dir),
        );
    }
}

// Same diff idiom as `sync_actors`, over the snapshot's pending-spawn list:
// a new id grows a beam-in ghost, a vanished id tears it down. The real
// actor arrives in the same snapshot its ghost entry disappears from, so the
// handoff is seamless.
pub fn sync_spawning_actors(
    commands: &mut Commands,
    ghosts: &mut ActorGhostMap,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    gameplay_config: &GameplayConfig,
    spawning_actors: &[(ActorId, SpawningActor)],
) {
    let update_ids: HashSet<ActorId> = spawning_actors.iter().map(|(id, _)| *id).collect();

    for (id, spawning) in spawning_actors {
        if let Some(entity) = ghosts.get(id) {
            let update = beam_in_ghost_state(gameplay_config, spawning);
            commands
                .entity(entity)
                .entry::<crate::vfx::BeamInGhost>()
                .and_modify(move |mut ghost| ghost.resync(update))
                .or_insert(update);
            continue;
        }
        let entity = spawn_actor_ghost(commands, asset_server, asset_set, gameplay_config, spawning);
        ghosts.insert(*id, entity);
    }

    ghosts.retain(|id, entity| {
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
    actors: &ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    id: ActorId,
    movement: ActorMovementState,
    face_dir: Option<f32>,
) {
    let Some(client_actor) = actors.get(&id) else {
        return;
    };

    let server_velocity = actor_movement_velocity(movement);
    commands.entity(client_actor.entity).insert((
        movement.move_intent,
        CharacterVerticalVelocity(movement.vertical_velocity),
    ));
    if let Some(face_dir) = face_dir {
        commands.entity(client_actor.entity).insert(FaceDirection(face_dir));
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
