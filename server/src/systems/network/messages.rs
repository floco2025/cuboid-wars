use bevy::prelude::*;

use super::broadcast::broadcast_to_others;
use crate::{net::ServerToClient, resources::PlayerMap};
use common::{
    constants::{ALWAYS_MULTI_SHOT, PROJECTILE_COOLDOWN_TIME},
    markers::{PlayerMarker, ProjectileMarker},
    physics::{CollisionWorld, PlayerMotion, ProjectileMotion, try_start_player_jump},
    protocol::*,
    spawning::calculate_projectile_spawns,
};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Dispatch messages from players who are already logged in to appropriate handlers.
pub fn dispatch_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    motions: &Query<&PlayerMotion, With<PlayerMarker>>,
    collision_world: &CollisionWorld,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            if let Some(player) = players.0.get(&id) {
                // Terminate the connection to enforce a single-login flow
                let _ = player.channel.send(ServerToClient::Close);
            }
        }
        ClientMessage::Logoff(msg) => {
            handle_logoff_message(commands, entity, id, msg, players);
        }
        ClientMessage::MoveInput(msg) => {
            trace!("{:?} move input: {:?}", id, msg);
            handle_move_input_message(commands, entity, id, msg, &*players, player_data);
        }
        ClientMessage::Jump(msg) => {
            trace!("{:?} jump: {:?}", id, msg);
            handle_jump_message(commands, entity, id, &*players, player_data, motions, collision_world);
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_message(commands, entity, id, msg, &*players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot_message(commands, entity, id, msg, players, time, player_data, collision_world);
        }
        ClientMessage::Echo(msg) => {
            handle_echo_message(id, msg, players);
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

// Handle logoff message.
fn handle_logoff_message(commands: &mut Commands, entity: Entity, id: PlayerId, _msg: CLogoff, players: &PlayerMap) {
    debug!("{:?} logged off", id);
    commands.entity(entity).despawn();

    // Broadcast graceful logoff to all other players
    broadcast_to_others(players, id, ServerMessage::Logoff(SLogoff { id, graceful: true }));
}

// Handle move-input message.
fn handle_move_input_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CMoveInput,
    players: &PlayerMap,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
) {
    // Update the player's input intent
    commands.entity(entity).insert(msg.move_input);

    // Get current position for reconciliation
    if let Ok((pos, _, _)) = player_data.get(entity) {
        // Broadcast move-input update with position to all other logged-in players
        broadcast_to_others(
            players,
            id,
            ServerMessage::MoveInput(SMoveInput {
                id,
                move_input: msg.move_input,
                pos: *pos,
            }),
        );
    }
}

fn handle_jump_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    players: &PlayerMap,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    motions: &Query<&PlayerMotion, With<PlayerMarker>>,
    collision_world: &CollisionWorld,
) {
    if players.0.get(&id).is_some_and(|info| info.stun_timer > 0.0) {
        return;
    }

    let Ok((pos, _, _)) = player_data.get(entity) else {
        return;
    };
    let Ok(motion) = motions.get(entity) else {
        return;
    };

    let mut next_motion = PlayerMotion {
        velocity: motion.velocity,
    };
    if !try_start_player_jump(&mut next_motion, collision_world, pos, pos.x, pos.z) {
        return;
    }

    commands.entity(entity).insert(PlayerMotion {
        velocity: next_motion.velocity,
    });
    broadcast_to_others(
        players,
        id,
        ServerMessage::Jump(SJump {
            id,
            pos: *pos,
            vy: next_motion.velocity.y,
        }),
    );
}

// Handle face direction message.
fn handle_face_message(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CFace, players: &PlayerMap) {
    // Update the player's face direction
    commands.entity(entity).insert(FaceDirection(msg.dir));

    broadcast_to_others(players, id, ServerMessage::Face(SFace { id, dir: msg.dir }));
}

// Handle shot message.
fn handle_shot_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
) {
    let now = time.elapsed_secs();

    let has_multi_shot = {
        let Some(player_info) = players.0.get_mut(&id) else {
            return;
        };

        if now - player_info.last_shot_time < PROJECTILE_COOLDOWN_TIME {
            return; // Throttled: ignore
        }

        player_info.last_shot_time = now;

        ALWAYS_MULTI_SHOT || player_info.multi_shot_power_up_timer > 0.0
    };

    // Update the shooter's face direction to exact facing direction
    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    // Spawn projectile(s) on server for hit detection
    if let Ok((pos, _, _)) = player_data.get(entity) {
        let spawns = calculate_projectile_spawns(pos, msg.face_dir, msg.face_pitch, has_multi_shot, collision_world);

        // Spawn each projectile
        for spawn_info in spawns {
            let proj_motion = ProjectileMotion::new(spawn_info.direction_yaw, spawn_info.direction_pitch);

            commands.spawn((
                ProjectileMarker,
                id, // Tag projectile with shooter's ID
                spawn_info.position,
                proj_motion,
            ));
        }
    }

    // Broadcast shot with face direction to all other logged-in players
    broadcast_to_others(
        players,
        id,
        ServerMessage::Shot(SShot {
            id,
            face_dir: msg.face_dir,
            face_pitch: msg.face_pitch,
        }),
    );
}

// Handle echo message.
fn handle_echo_message(id: PlayerId, msg: CEcho, players: &PlayerMap) {
    trace!("{:?} echo: {:?}", id, msg);
    if let Some(player_info) = players.0.get(&id) {
        let echo_msg = ServerMessage::Echo(SEcho {
            timestamp_nanos: msg.timestamp_nanos,
        });
        let _ = player_info.channel.send(ServerToClient::Send(echo_msg));
    }
}
