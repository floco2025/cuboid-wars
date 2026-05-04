use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    actors::{ActorInfo, ActorMap, spawn_actor},
    config::{AssetSet, RenderSettings},
    network::RoundTripTime,
    vfx::spawn_actor_explosion,
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{
        Actor, ActorId, ActorMarker, ActorMoveIntent, ActorMovementState, FaceDirection, Position, SActorDestroyed,
        SActorHit, SActorMoveIntent, SActorTeleport,
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
        actors.0.insert(
            *id,
            ActorInfo {
                entity,
                kind: actor.kind.clone(),
            },
        );
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

// Server despawns the entity; client cleanup happens when the next snapshot
// arrives without this actor (sync_actors removes any actor not in the
// update). No need to mutate the entity here — just play the VFX.
//
// Kind isn't on the wire `SActorDestroyed`; we recover it from the local
// `ActorMap` where the kind was recorded when this actor was first seen.
pub fn handle_actor_destroyed_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    actors: &ActorMap,
    gameplay_config: &GameplayConfig,
    msg: SActorDestroyed,
) {
    let Some(info) = actors.0.get(&msg.id) else {
        return;
    };
    spawn_actor_explosion(
        commands,
        asset_server,
        meshes,
        materials,
        asset_set,
        gameplay_config,
        &info.kind,
        msg.pos,
    );
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(&info.kind, "explodes").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

pub fn handle_actor_hit_message(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    msg: SActorHit,
) {
    trace!("{:?} was hit", msg.id);
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("hit_actor").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
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
