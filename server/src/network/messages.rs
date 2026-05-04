use bevy::prelude::*;

use super::broadcast::broadcast_to_others;
use crate::{net::ServerToClient, resources::PlayerMap};
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_MULTI_SHOT, PROJECTILE_COOLDOWN_TIME},
    physics::{CharacterVerticalVelocity, CollisionWorld, ProjectileMarker, ProjectileMotion, try_start_player_jump},
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
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            if let Some(player) = players.get(&id) {
                // Terminate the connection to enforce a single-login flow
                let _ = player.channel.send(ServerToClient::Close);
            }
        }
        ClientMessage::Logoff(msg) => {
            handle_logoff_message(commands, entity, id, msg, players);
        }
        ClientMessage::PlayerMoveIntent(msg) => {
            trace!("{:?} move intent: {:?}", id, msg);
            handle_move_intent_message(commands, entity, id, msg, &*players, player_data, motions);
        }
        ClientMessage::Jump(msg) => {
            trace!("{:?} jump: {:?}", id, msg);
            handle_jump_message(
                commands,
                entity,
                id,
                &*players,
                player_data,
                motions,
                collision_world,
                gameplay_config,
            );
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_message(commands, entity, id, msg, &*players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot_message(
                commands,
                entity,
                id,
                msg,
                players,
                time,
                player_data,
                collision_world,
                gameplay_config,
            );
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
fn handle_move_intent_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CPlayerMoveIntent,
    players: &PlayerMap,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
) {
    // Update the player's input intent
    commands.entity(entity).insert(msg.move_intent);

    // Get current movement state for reconciliation.
    if let (Ok((pos, _, _, _)), Ok(motion)) = (player_data.get(entity), motions.get(entity)) {
        // Broadcast move-input update with position to all other logged-in players
        broadcast_to_others(
            players,
            id,
            ServerMessage::PlayerMoveIntent(SPlayerMoveIntent {
                id,
                movement: PlayerMovementState::new(*pos, msg.move_intent, motion.0),
            }),
        );
    }
}

fn handle_jump_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    players: &PlayerMap,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) {
    if players.get(&id).is_some_and(|info| info.stun_timer > 0.0) {
        return;
    }

    let Ok((pos, move_intent, _, _)) = player_data.get(entity) else {
        return;
    };
    let Ok(motion) = motions.get(entity) else {
        return;
    };

    let mut next_vertical_velocity = motion.0;
    if !try_start_player_jump(
        &mut next_vertical_velocity,
        collision_world,
        gameplay_config.player.physics(),
        pos,
        pos.x,
        pos.z,
    ) {
        return;
    }

    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(next_vertical_velocity));
    broadcast_to_others(
        players,
        id,
        ServerMessage::Jump(SJump {
            id,
            movement: PlayerMovementState::new(*pos, *move_intent, next_vertical_velocity),
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
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) {
    let now = time.elapsed_secs();

    let has_multi_shot = {
        let Some(player_info) = players.get_mut(&id) else {
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
    if let Ok((pos, _, _, _)) = player_data.get(entity) {
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_dir,
            msg.face_pitch,
            has_multi_shot,
            gameplay_config.player.eye_height(),
            collision_world,
        );

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
    if let Some(player_info) = players.get(&id) {
        let echo_msg = ServerMessage::Echo(SEcho {
            timestamp_nanos: msg.timestamp_nanos,
        });
        let _ = player_info.channel.send(ServerToClient::Send(echo_msg));
    }
}
