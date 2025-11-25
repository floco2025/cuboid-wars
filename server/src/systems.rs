#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use rand::Rng as _;

use crate::{
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, PlayerInfo, PlayerMap},
};
use common::protocol::*;

// ============================================================================
// Accept Connections System
// ============================================================================

pub fn accept_connections_system(
    mut commands: Commands,
    mut from_accept: ResMut<FromAcceptChannel>,
    mut players: ResMut<PlayerMap>,
) {
    while let Ok((id, to_client)) = from_accept.try_recv() {
        debug!("{:?} connected", id);
        let entity = commands.spawn(id).id();
        players.0.insert(
            id,
            PlayerInfo {
                entity,
                logged_in: false,
                channel: to_client,
            },
        );
    }
}

// ============================================================================
// Client Event Processing System
// ============================================================================

// NOTE: Must run after accept_connections_system with apply_deferred in between, otherwise entities
// for the messages might not be spawned yet.
pub fn process_client_message_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position>,
    velocities: Query<&Velocity>,
    rotations: Query<&Rotation>,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        let Some(player_info) = players.0.get(&id) else {
            error!("received event for unknown {:?}", id);
            continue;
        };

        match event {
            ClientToServer::Disconnected => {
                let was_logged_in = player_info.logged_in;
                let entity = player_info.entity;
                players.0.remove(&id);
                commands.entity(entity).despawn();

                debug!("{:?} disconnected (logged_in: {})", id, was_logged_in);

                // Broadcast logoff to all other logged-in players if they were logged in
                if was_logged_in {
                    let logoff_msg = ServerMessage::Logoff(SLogoff { id, graceful: false });
                    for (other_id, other_info) in players.0.iter() {
                        if *other_id != id && other_info.logged_in {
                            let _ = other_info.channel.send(ServerToClient::Send(logoff_msg.clone()));
                        }
                    }
                }
            }
            ClientToServer::Message(message) => {
                let is_logged_in = player_info.logged_in;
                if is_logged_in {
                    process_message_logged_in(&mut commands, player_info.entity, id, message, &players);
                } else {
                    process_message_not_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &positions,
                        &velocities,
                        &rotations,
                        &mut players,
                    );
                }
            }
        }
    }
}

// ============================================================================
// Process Messages
// ============================================================================

fn process_message_not_logged_in(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    positions: &Query<&Position>,
    velocities: &Query<&Velocity>,
    rotations: &Query<&Rotation>,
    players: &mut ResMut<PlayerMap>,
) {
    match msg {
        ClientMessage::Login(_) => {
            debug!("{:?} logged in", id);

            let player_info = players
                .0
                .get_mut(&id)
                .expect("process_message_not_logged_in called for unknown player");

            // Send Init to the connecting player (just their ID)
            let init_msg = ServerMessage::Init(SInit { id });
            if let Err(e) = player_info.channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let mut rng = rand::rng();
            let pos = Position {
                x: rng.random_range(-800_000..=800_000),
                y: rng.random_range(-800_000..=800_000),
            };

            // Initial velocity and rotation for the new player
            let vel = Velocity { x: 0.0, y: 0.0 };
            let rot = Rotation { yaw: 0.0 };

            // Construct player data
            let player = Player { pos, vel, rot };

            // Mark as logged in and clone channel for later use
            player_info.logged_in = true;
            let channel = player_info.channel.clone();

            // Construct the initial Update for the new player
            let mut all_players: Vec<(PlayerId, Player)> = players
                .0
                .iter()
                .filter_map(|(player_id, info)| {
                    // Skip the new player here because their components aren't in ECS yet. Also
                    // skip all players that are not logged in yet.
                    if *player_id == id || !info.logged_in {
                        return None;
                    }
                    let pos = positions.get(info.entity).ok()?;
                    let vel = velocities.get(info.entity).ok()?;
                    let rot = rotations.get(info.entity).ok()?;
                    Some((
                        *player_id,
                        Player {
                            pos: *pos,
                            vel: *vel,
                            rot: *rot,
                        },
                    ))
                })
                .collect();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate { players: all_players });
            channel.send(ServerToClient::Send(update_msg)).ok();

            // Now update entity: add Position + Velocity + Rotation
            commands.entity(entity).insert((pos, vel, rot));

            // Broadcast Login to all other logged-in players
            let login_msg = ServerMessage::Login(SLogin { id, player });
            for (other_id, other_info) in players.0.iter() {
                if *other_id != id && other_info.logged_in {
                    let _ = other_info.channel.send(ServerToClient::Send(login_msg.clone()));
                }
            }
        }
        _ => {
            warn!(
                "{:?} sent non-login message before authenticating (likely out-of-order delivery)",
                id
            );
            // Don't despawn - Init message will likely arrive soon
        }
    }
}

fn process_message_logged_in(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &PlayerMap,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            commands.entity(entity).despawn();
        }
        ClientMessage::Logoff(_) => {
            debug!("{:?} logged off", id);
            commands.entity(entity).despawn();

            // Broadcast graceful logoff to all other players
            let broadcast_msg = ServerMessage::Logoff(SLogoff { id, graceful: true });
            for (other_id, other_info) in players.0.iter() {
                if *other_id != id && other_info.logged_in {
                    let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
                }
            }
        }
        ClientMessage::Velocity(msg) => {
            trace!("{:?} velocity: {:?}", id, msg);
            handle_velocity(commands, entity, id, msg, players);
        }
        ClientMessage::Rotation(msg) => {
            trace!("{:?} rotation: {:?}", id, msg);
            handle_rotation(commands, entity, id, msg, players);
        }
    }
}

// ============================================================================
// Movement Handlers
// ============================================================================

fn handle_velocity(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CVelocity, players: &PlayerMap) {
    // Update the player's velocity
    commands.entity(entity).insert(msg.vel);

    // Broadcast velocity update to all other logged-in players
    let broadcast_msg = ServerMessage::PlayerVelocity(SVelocity { id, vel: msg.vel });
    for (other_id, other_info) in players.0.iter() {
        if *other_id != id && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
        }
    }
}

fn handle_rotation(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CRotation, players: &PlayerMap) {
    // Update the player's rotation
    commands.entity(entity).insert(msg.rot);

    // Broadcast rotation update to all other logged-in players
    let broadcast_msg = ServerMessage::PlayerRotation(SRotation { id, rot: msg.rot });
    for (other_id, other_info) in players.0.iter() {
        if *other_id != id && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
        }
    }
}

/// Broadcast authoritative kinematics (position+velocity+rotation) once per second
pub fn broadcast_state_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    positions: Query<&Position>,
    velocities: Query<&Velocity>,
    rotations: Query<&Rotation>,
    players: Res<PlayerMap>,
) {
    const BROADCAST_INTERVAL: f32 = 10.0;

    *timer += time.delta_secs();
    if *timer < BROADCAST_INTERVAL {
        return;
    }
    *timer = 0.0;

    // Collect all logged-in players
    let all_players: Vec<(PlayerId, Player)> = players
        .0
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let pos = positions.get(info.entity).ok()?;
            let vel = velocities.get(info.entity).ok()?;
            let rot = rotations.get(info.entity).ok()?;
            Some((
                *player_id,
                Player {
                    pos: *pos,
                    vel: *vel,
                    rot: *rot,
                },
            ))
        })
        .collect();

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate { players: all_players });
    debug!("broadcasting update: {:?}", msg);
    for info in players.0.values() {
        if info.logged_in {
            let _ = info.channel.send(ServerToClient::Send(msg.clone()));
        }
    }
}
