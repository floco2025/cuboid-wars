//! A passive view of an experiment's completed ticks. This adapter never runs
//! player physics or feeds observations back into a live movement owner.
use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    physics::{CharacterVerticalVelocity, player_control_velocity},
    protocol::*,
};

use super::{LastSnapshotTick, context::ServerMessageContext, routing::route_server_message};
use crate::{
    actors::{ActorAnimationVelocity, ActorMap},
    characters::PreviousTickPosition,
    players::{LocalPlayerInfo, PlayerAnimationMotion, PlayerMap, PlayerMotionBundle},
    projectiles::ProjectileAssets,
    vfx::FireworkShow,
};

#[derive(Resource)]
pub struct PlaybackMode;

pub(crate) fn live_gameplay(playback: Option<Res<PlaybackMode>>) -> bool {
    playback.is_none()
}

pub fn install_playback(app: &mut App) {
    app.insert_resource(PlaybackMode);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
}

pub struct PlaybackFrame {
    pub snapshot: SSnapshot,
    pub projectiles: Vec<(u64, Position)>,
    pub cues: Vec<ServerMessage>,
    pub reset: bool,
}

#[derive(Component)]
struct PlaybackProjectileMarker(u64);

pub fn apply_playback_frame(world: &mut World, frame: PlaybackFrame) {
    assert!(world.contains_resource::<PlaybackMode>());
    // Preserve inspection orientation through relocations and restarts. The
    // initial spawn still consumes the usual --look option.
    let view = (world.resource::<PlayerMap>().iter().next().is_some()).then(|| {
        let local = world.resource::<LocalPlayerInfo>();
        (local.stored_yaw, local.stored_pitch)
    });
    if frame.reset {
        clear_bodies(world, &frame.snapshot);
    }
    let mut state = SystemState::<(Commands, ServerMessageContext)>::new(world);
    {
        let (mut commands, mut context) = state.get_mut(world).expect("client presentation resources installed");
        // Local observation is not a wire stream: aiming may change the owner
        // without advancing the server tick, and reset starts tick numbering over.
        context.clocks.last_snapshot_tick.0 = None;
        route_server_message(
            ServerMessage::Snapshot(frame.snapshot.clone()),
            &mut commands,
            &mut context,
        );
        for cue in frame.cues {
            route_server_message(cue, &mut commands, &mut context);
        }
    }
    state.apply(world);
    if let Some((yaw, pitch)) = view {
        let mut local = world.resource_mut::<LocalPlayerInfo>();
        local.stored_yaw = yaw;
        local.stored_pitch = pitch;
    }
    world.resource_mut::<ServerTick>().0 = frame.snapshot.tick;
    place_bodies(world, &frame.snapshot);
    place_projectiles(world, &frame.projectiles);
}

fn clear_bodies(world: &mut World, snapshot: &SSnapshot) {
    let mut empty = snapshot.clone();
    empty.players.clear();
    empty.actors.clear();
    empty.spawning_actors.clear();
    empty.items.clear();
    empty.missiles.clear();
    empty.portals.clear();
    world.resource_mut::<LastSnapshotTick>().0 = None;
    let mut state = SystemState::<(Commands, ServerMessageContext)>::new(world);
    {
        let (mut commands, mut context) = state.get_mut(world).expect("client presentation resources installed");
        route_server_message(ServerMessage::Snapshot(empty), &mut commands, &mut context);
        // The live client keeps the local body hidden through death. A new
        // experiment must also discard its generation and checkpoint guards.
        for (_, info) in context.players.iter() {
            commands.entity(info.entity).despawn();
        }
        *context.players = PlayerMap::default();
        *context.firework_show = FireworkShow::default();
    }
    state.apply(world);
}

fn place_bodies(world: &mut World, snapshot: &SSnapshot) {
    let settings = world.resource::<MapSettings>().movement.clone();
    for (id, player) in &snapshot.players {
        let entity = world.resource::<PlayerMap>().get(id).expect("snapshot player").entity;
        let movement = &player.movement;
        let velocity = player_control_velocity(
            movement.move_intent,
            &settings,
            player.power_ups[PowerUpKind::Speed.index()],
            player.stunned,
        );
        world.entity_mut(entity).insert((
            movement.pos,
            PreviousTickPosition(movement.pos),
            PlayerMotionBundle::from(movement),
            PlayerAnimationMotion {
                support: movement.support,
                velocity: velocity.with_y(movement.vertical_velocity),
            },
        ));
    }
    let delta = world.resource::<Time>().delta_secs();
    for (id, actor) in &snapshot.actors {
        let entity = world.resource::<ActorMap>().get(id).expect("snapshot actor").entity;
        let movement = &actor.movement;
        let previous = *world.get::<Position>(entity).expect("actor position");
        let velocity = if delta > 0.0 {
            (Vec3::from(movement.pos) - Vec3::from(previous)) / delta
        } else {
            Vec3::ZERO
        };
        world.entity_mut(entity).insert((
            movement.pos,
            FaceYaw(movement.face_yaw),
            movement.move_intent,
            CharacterVerticalVelocity(movement.vertical_velocity),
            movement.support,
            ActorAnimationVelocity(velocity),
        ));
    }
}

fn place_projectiles(world: &mut World, projectiles: &[(u64, Position)]) {
    let existing: Vec<_> = world
        .query::<(Entity, &PlaybackProjectileMarker)>()
        .iter(world)
        .map(|(entity, marker)| (entity, marker.0))
        .collect();
    for &(entity, id) in &existing {
        if let Some((_, position)) = projectiles.iter().find(|(shot, _)| *shot == id) {
            world
                .entity_mut(entity)
                .insert(Transform::from_translation((*position).into()));
        } else {
            world.despawn(entity);
        }
    }
    for &(id, position) in projectiles {
        if !existing.iter().any(|(_, present)| *present == id) {
            let assets = world.resource::<ProjectileAssets>();
            let mesh = assets.mesh.clone();
            let material = assets.material.clone();
            world.spawn((
                PlaybackProjectileMarker(id),
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(position.into()),
            ));
        }
    }
}

#[cfg(test)]
#[path = "tests/playback.rs"]
mod tests;
