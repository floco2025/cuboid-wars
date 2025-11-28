use bevy::prelude::*;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

use super::effects::{CameraShake, CuboidShake};
use crate::{
    constants::ECHO_INTERVAL,
    net::{ClientToServer, ServerToClient},
    resources::{
        ClientToServerChannel, MyPlayerId, PastPosVel, PlayerInfo, PlayerMap, RoundTripTime, ServerToClientChannel,
        WallConfig,
    },
    spawning::{spawn_player, spawn_projectile_for_player},
};
use common::protocol::{CEcho, ClientMessage, FaceDirection, PlayerId, Position, ServerMessage};

// ============================================================================
// Network Message Processing
// ============================================================================

// System to process messages from the server
pub fn process_server_events_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut player_map: ResMut<PlayerMap>,
    mut rtt: ResMut<RoundTripTime>,
    mut rtt_measurements: Local<VecDeque<f64>>,
    mut past_pos_vel: ResMut<PastPosVel>,
    player_pos_query: Query<&Position, With<PlayerId>>,
    player_face_query: Query<(&Position, &common::protocol::FaceDirection), With<PlayerId>>,
    camera_query: Query<Entity, With<Camera3d>>,
    my_player_id: Option<Res<MyPlayerId>>,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                if my_player_id.is_some() {
                    process_message_logged_in(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut player_map,
                        &mut rtt,
                        &mut rtt_measurements,
                        &mut past_pos_vel,
                        &player_pos_query,
                        &player_face_query,
                        &camera_query,
                        my_player_id.as_ref().unwrap().0,
                        &message,
                    );
                } else {
                    process_message_not_logged_in(&mut commands, &message);
                }
            }
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn process_message_not_logged_in(commands: &mut Commands, msg: &ServerMessage) {
    match msg {
        ServerMessage::Init(init_msg) => {
            debug!("received Init: my_id={:?}", init_msg.id);

            // Store player ID as resource
            commands.insert_resource(MyPlayerId(init_msg.id));

            // Store walls configuration
            commands.insert_resource(WallConfig {
                walls: init_msg.walls.clone(),
            });

            // Note: We don't spawn anything here. The first SUpdate will contain
            // all players including ourselves and will trigger spawning via the
            // Update message handler.
        }
        _ => {
            warn!("received non-Init message before Init (out-of-order delivery)");
        }
    }
}

fn process_message_logged_in(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &mut ResMut<RoundTripTime>,
    rtt_measurements: &mut VecDeque<f64>,
    past_pos_vel: &mut ResMut<PastPosVel>,
    player_pos_query: &Query<&Position, With<PlayerId>>,
    player_face_query: &Query<(&Position, &FaceDirection), With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: &ServerMessage,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(msg) => {
            debug!("{:?} logged in", msg.id);
            // Spawn the new player if not already in player_map
            if !players.0.contains_key(&msg.id) {
                let entity = spawn_player(
                    commands,
                    meshes,
                    materials,
                    msg.id.0,
                    &msg.player.pos,
                    msg.player.speed.to_velocity(),
                    msg.player.face_dir,
                    false, // Never local (server doesn't send our own login back)
                );
                players.0.insert(msg.id, PlayerInfo { entity, hits: 0 });
            }
        }
        ServerMessage::Logoff(msg) => {
            debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
            // Remove from player map and despawn entity
            if let Some(player) = players.0.remove(&msg.id) {
                commands.entity(player.entity).despawn();
            }
        }
        ServerMessage::Speed(msg) => {
            trace!("{:?} speed: {:?}", msg.id, msg);
            // Update player speed using player_map
            if let Some(player) = players.0.get(&msg.id) {
                let velocity = msg.speed.to_velocity();
                commands.entity(player.entity).insert(velocity);
            } else {
                warn!("received speed for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Face(msg) => {
            trace!("{:?} face direction: {}", msg.id, msg.dir);
            // Update player face direction using player_map
            if let Some(player) = players.0.get(&msg.id) {
                commands.entity(player.entity).insert(FaceDirection(msg.dir));
            } else {
                warn!("received face direction for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Shot(msg) => {
            trace!("{:?} shot: {:?}", msg.id, msg);
            // Update the shooter's face direction first to sync exact facing direction
            if let Some(player) = players.0.get(&msg.id) {
                commands.entity(player.entity).insert(FaceDirection(msg.face_dir));
                // Spawn projectile for this player
                spawn_projectile_for_player(commands, meshes, materials, player_face_query, player.entity);
            } else {
                warn!("received shot from non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Update(msg) => {
            //trace!("update: {:?}", msg);

            // Collect player IDs in this Update message
            let update_players: std::collections::HashSet<PlayerId> = msg.players.iter().map(|(id, _)| *id).collect();

            // Spawn missing players (in Update but not in player_map)
            for (id, player) in &msg.players {
                if !players.0.contains_key(id) {
                    let is_local = my_player_id.0 == (*id).0;
                    debug!("spawning player {:?} from Update (is_local: {})", id, is_local);
                    let entity = spawn_player(
                        commands,
                        meshes,
                        materials,
                        id.0,
                        &player.pos,
                        player.speed.to_velocity(),
                        player.face_dir,
                        is_local,
                    );

                    if is_local {
                        // Initialize past position and velocity for local player
                        past_pos_vel.pos = player.pos;
                        past_pos_vel.vel = player.speed.to_velocity();
                        past_pos_vel.timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();

                        // Initialize camera rotation to match local player's spawn rotation
                        if let Ok(camera_entity) = camera_query.single() {
                            // Camera rotation needs π offset because camera looks along -Z in local space
                            // but face_dir assumes looking along +Z
                            let camera_rotation = player.face_dir + std::f32::consts::PI;
                            commands.entity(camera_entity).insert(
                                Transform::from_xyz(player.pos.x, 2.5, player.pos.z + 3.0)
                                    .with_rotation(Quat::from_rotation_y(camera_rotation)),
                            );
                        }
                    }

                    players.0.insert(
                        *id,
                        PlayerInfo {
                            entity,
                            hits: player.hits,
                        },
                    );
                }
            }

            // Despawn players that no longer exist (in player_map but not in Update)
            let to_remove: Vec<PlayerId> = players
                .0
                .keys()
                .filter(|id| !update_players.contains(id))
                .copied()
                .collect();

            for id in to_remove {
                if let Some(player) = players.0.remove(&id) {
                    warn!("despawning player {:?} from Update", id);
                    commands.entity(player.entity).despawn();
                }
            }

            // Update all players with new state
            for (id, server_player) in &msg.players {
                if let Some(client_player) = players.0.get_mut(id) {
                    // if *id == my_player_id {
                    //     // For local player: compare pred position vs server position
                    //     if let Ok(client_pos) = player_pos_query.get(client_player.entity) {
                    //         // Calculate where we should be based on past position + speed over RTT
                    //         let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
                    //         let elapsed_since_past = (now - past_pos_vel.timestamp) as f32;
                    //         let past_pred = Position {
                    //             x: past_pos_vel.pos.x + past_pos_vel.vel.x * elapsed_since_past,
                    //             y: 0.0,
                    //             z: past_pos_vel.pos.z + past_pos_vel.vel.z * elapsed_since_past,
                    //         };

                    //         // Calculate predicted position from server pos + speed over half RTT
                    //         let server_speed = server_player.speed.to_velocity();
                    //         let half_rtt = (rtt.rtt / 2.0) as f32;
                    //         let server_pred = Position {
                    //             x: server_player.pos.x + server_speed.x * half_rtt,
                    //             y: 0.0,
                    //             z: server_player.pos.z + server_speed.z * half_rtt,
                    //         };

                    //         // Calculate signed distances projected along server movement direction
                    //         // Positive = ahead in movement direction, Negative = behind
                    //         let move_dir_sin = server_player.speed.move_dir.sin();
                    //         let move_dir_cos = server_player.speed.move_dir.cos();

                    //         let server_to_current_x = server_player.pos.x - client_pos.x;
                    //         let server_to_current_z = server_player.pos.z - client_pos.z;
                    //         let server_to_current_signed =
                    //             server_to_current_x * move_dir_sin + server_to_current_z * move_dir_cos;

                    //         let current_to_server_pred_x = server_pred.x - client_pos.x;
                    //         let current_to_server_pred_z = server_pred.z - client_pos.z;
                    //         let current_to_server_pred_signed =
                    //             current_to_server_pred_x * move_dir_sin + current_to_server_pred_z * move_dir_cos;

                    //         let server_to_server_pred_x = server_pred.x - server_player.pos.x;
                    //         let server_to_server_pred_z = server_pred.z - server_player.pos.z;
                    //         let server_to_server_pred_signed =
                    //             server_to_server_pred_x * move_dir_sin + server_to_server_pred_z * move_dir_cos;

                    //         let server_to_past_pred_x = past_pred.x - server_player.pos.x;
                    //         let server_to_past_pred_z = past_pred.z - server_player.pos.z;
                    //         let server_to_past_pred_signed =
                    //             server_to_past_pred_x * move_dir_sin + server_to_past_pred_z * move_dir_cos;

                    //         debug!(
                    //             "s2c={:+.2} s2pp={:+.2} s2sp={:+.2} c2sp={:+.2} {:?}",
                    //             server_to_current_signed,
                    //             server_to_past_pred_signed,
                    //             server_to_server_pred_signed,
                    //             current_to_server_pred_signed,
                    //             server_player.speed.speed_level,
                    //         );

                    //         // Apply server correction
                    //         commands
                    //             .entity(client_player.entity)
                    //             .insert((server_player.pos, server_player.speed.to_velocity()));
                    //     } else {
                    //         // Other players: always accept server state
                    //         commands
                    //             .entity(client_player.entity)
                    //             .insert((server_player.pos, server_player.speed.to_velocity()));
                    //     }

                    commands
                        .entity(client_player.entity)
                        .insert((server_player.pos, server_player.speed.to_velocity()));
                    client_player.hits = server_player.hits;
                }
            }
        }
        ServerMessage::Hit(msg) => {
            debug!("player {:?} was hit", msg.id);
            // Check if it's the local player
            if msg.id == my_player_id {
                // Shake camera for local player
                if let Ok(camera_entity) = camera_query.single() {
                    commands.entity(camera_entity).insert(CameraShake {
                        timer: Timer::from_seconds(0.3, TimerMode::Once),
                        intensity: 3.0,
                        direction_x: msg.hit_dir_x,
                        direction_z: msg.hit_dir_z,
                        offset_x: 0.0,
                        offset_y: 0.0,
                        offset_z: 0.0,
                    });
                }
            } else {
                // Shake cuboid for other players
                if let Some(player) = players.0.get(&msg.id) {
                    commands.entity(player.entity).insert(CuboidShake {
                        timer: Timer::from_seconds(0.3, TimerMode::Once),
                        intensity: 0.3,
                        direction_x: msg.hit_dir_x,
                        direction_z: msg.hit_dir_z,
                        offset_x: 0.0,
                        offset_z: 0.0,
                    });
                }
            }
        }
        ServerMessage::Echo(msg) => {
            if rtt.pending_timestamp != 0.0 && msg.timestamp == rtt.pending_timestamp {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
                let measured_rtt = now - rtt.pending_timestamp;
                rtt.pending_timestamp = 0.0;

                // Add measurement and keep only last 10
                rtt_measurements.push_back(measured_rtt);
                if rtt_measurements.len() > 10 {
                    rtt_measurements.pop_front();
                }

                // Calculate average
                let sum: f64 = rtt_measurements.iter().sum();
                rtt.rtt = sum / rtt_measurements.len() as f64;
            }
        }
    }
}

// ============================================================================
// Echo/Ping System
// ============================================================================

// System to send echo requests every ECHO_INTERVAL seconds
pub fn echo_system(
    time: Res<Time>,
    mut rtt: ResMut<RoundTripTime>,
    to_server: Res<ClientToServerChannel>,
    mut timer: Local<f32>,
    mut initialized: Local<bool>,
) {
    // Initialize timer to send first echo after 1 second
    if !*initialized {
        *timer = ECHO_INTERVAL - 1.0;
        *initialized = true;
    }

    let delta = time.delta_secs();
    *timer += delta;

    // Send echo request every ECHO_INTERVAL seconds
    if *timer >= ECHO_INTERVAL {
        *timer = 0.0;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        rtt.pending_timestamp = timestamp;
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Echo(CEcho { timestamp })));
    }
}
