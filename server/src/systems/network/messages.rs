use bevy::prelude::*;

use crate::net::ServerToClient;
use crate::resources::PlayerMap;
use common::protocol::MapLayout;
use common::{
    collision::Projectile,
    constants::PROJECTILE_COOLDOWN_TIME,
    markers::{PlayerMarker, ProjectileMarker},
    protocol::*,
    spawning::calculate_projectile_spawns,
};

use super::broadcast::broadcast_to_others;

// ============================================================================
// Message Processing for Logged-in Players
// ============================================================================

/// Process messages from players who are already logged in.
pub fn process_message_logged_in(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    map_layout: &MapLayout,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            if let Some(player) = players.0.get(&id) {
                // Terminate the connection to enforce a single-login flow
                let _ = player.channel.send(ServerToClient::Close);
            }
        }
        ClientMessage::Logoff(_) => {
            debug!("{:?} logged off", id);
            commands.entity(entity).despawn();

            // Broadcast graceful logoff to all other players
            broadcast_to_others(players, id, ServerMessage::Logoff(SLogoff { id, graceful: true }));
        }
        ClientMessage::Speed(msg) => {
            trace!("{:?} speed: {:?}", id, msg);
            handle_speed(commands, entity, id, msg, &*players, player_data);
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_direction(commands, entity, id, msg, &*players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot(commands, entity, id, msg, players, time, player_data, map_layout);
        }
        ClientMessage::Echo(msg) => {
            trace!("{:?} echo: {:?}", id, msg);
            if let Some(player_info) = players.0.get(&id) {
                let echo_msg = ServerMessage::Echo(SEcho {
                    timestamp_nanos: msg.timestamp_nanos,
                });
                let _ = player_info.channel.send(ServerToClient::Send(echo_msg));
            }
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

/// Handle player speed changes.
fn handle_speed(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CSpeed,
    players: &PlayerMap,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
) {
    // Update the player's speed
    commands.entity(entity).insert(msg.speed);

    // Get current position for reconciliation
    if let Ok((pos, _, _)) = player_data.get(entity) {
        // Broadcast speed update with position to all other logged-in players
        broadcast_to_others(
            players,
            id,
            ServerMessage::Speed(SSpeed {
                id,
                speed: msg.speed,
                pos: *pos,
            }),
        );
    }
}

/// Handle player face direction changes.
fn handle_face_direction(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CFace, players: &PlayerMap) {
    // Update the player's face direction
    commands.entity(entity).insert(FaceDirection(msg.dir));

    broadcast_to_others(players, id, ServerMessage::Face(SFace { id, dir: msg.dir }));
}

/// Handle player shooting.
fn handle_shot(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    map_layout: &MapLayout,
) {
    use common::constants::ALWAYS_MULTI_SHOT;

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
        // Calculate valid projectile spawn positions (all_walls excludes roof-edge guards)
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_dir,
            msg.face_pitch,
            has_multi_shot,
            &map_layout.lower_walls,
            &map_layout.ramps,
            &map_layout.roofs,
        );

        // Spawn each projectile
        for spawn_info in spawns {
            let projectile = Projectile::new(spawn_info.direction_yaw, spawn_info.direction_pitch);

            commands.spawn((
                ProjectileMarker,
                id, // Tag projectile with shooter's ID
                spawn_info.position,
                projectile,
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
