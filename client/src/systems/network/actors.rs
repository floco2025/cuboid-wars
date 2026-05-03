use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    config::{AssetSet, RenderSettings},
    resources::{ActorInfo, ActorMap, RoundTripTime},
    spawning::spawn_actor,
};
use common::{
    config::GameplayConfig,
    markers::ActorMarker,
    physics::CharacterVerticalVelocity,
    protocol::{
        Actor, ActorId, ActorMoveIntent, ActorMovementState, FaceDirection, Position, SActorMoveIntent, SActorTeleport,
    },
};

pub fn sync_actors(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    actors: &mut ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    server_actors: &[(ActorId, Actor)],
) {
    let update_ids: HashSet<ActorId> = server_actors.iter().map(|(id, _)| *id).collect();

    for (id, actor) in server_actors {
        if actors.0.contains_key(id) {
            continue;
        }

        let entity = spawn_actor(
            commands,
            asset_server,
            meshes,
            materials,
            images,
            graphs,
            asset_set,
            render_settings,
            gameplay_config,
            *id,
            actor,
        );
        actors.0.insert(*id, ActorInfo { entity });
    }

    actors.0.retain(|id, actor| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(actor.entity).despawn();
            false
        }
    });

    for (id, server_actor) in server_actors {
        if let Some(client_actor) = actors.0.get(id) {
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

pub fn handle_actor_teleport_message(commands: &mut Commands, actors: &ResMut<ActorMap>, msg: SActorTeleport) {
    let Some(client_actor) = actors.0.get(&msg.id) else {
        return;
    };

    commands.entity(client_actor.entity).insert((
        msg.movement.pos,
        msg.movement.move_intent,
        CharacterVerticalVelocity(msg.movement.vertical_velocity),
    ));
    commands.entity(client_actor.entity).remove::<ServerReconciliation>();
}

fn apply_actor_movement_state(
    commands: &mut Commands,
    actors: &ResMut<ActorMap>,
    rtt: &ResMut<RoundTripTime>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    id: ActorId,
    movement: ActorMovementState,
    face_dir: Option<f32>,
) {
    let Some(client_actor) = actors.0.get(&id) else {
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
        commands.entity(client_actor.entity).insert(ServerReconciliation {
            client_pos: *client_pos,
            server_pos: movement.pos,
            server_velocity,
            timer: 0.0,
            rtt: rtt.rtt.as_secs_f32(),
        });
    }
}

fn actor_movement_velocity(movement: ActorMovementState) -> Vec3 {
    let mut velocity = movement.move_intent.to_horizontal_velocity();
    velocity.y = movement.vertical_velocity;
    velocity
}
