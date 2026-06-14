use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    actors::{ActorInfo, ActorMap, spawn_actor},
    config::{AssetSet, ClientSettings},
    network::RoundTripTime,
    vfx::spawn_actor_explosion,
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{
        Actor, ActorId, ActorMarker, ActorMoveIntent, ActorMovementState, FaceDirection, Position, SActorDeath,
        SActorHit, SActorMoveIntent,
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
pub fn handle_actor_death_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    actors: &mut ResMut<ActorMap>,
    players: &mut crate::players::PlayerMap,
    gameplay_config: &GameplayConfig,
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
    commands.entity(info.entity).despawn();
}

pub fn handle_actor_hit_message(
    commands: &mut Commands,
    actors: &ActorMap,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    msg: SActorHit,
) {
    trace!("{:?} was hit", msg.id);
    // Early-apply the post-hit health so the floating health bar drops on
    // the impact tick instead of waiting for the next snapshot.
    if let Some(info) = actors.get(&msg.id) {
        commands.entity(info.entity).insert(msg.health);
    }
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
