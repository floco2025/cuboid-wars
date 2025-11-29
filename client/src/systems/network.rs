#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::{
    collections::{HashSet, VecDeque},
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
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

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
    player_face_query: Query<(&Position, &FaceDirection), With<PlayerId>>,
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
                if let Some(my_id) = my_player_id.as_ref() {
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
                        my_id.0,
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
        ServerMessage::Login(login) => handle_login_message(commands, meshes, materials, players, login),
        ServerMessage::Logoff(logoff) => handle_logoff_message(commands, players, logoff),
        ServerMessage::Speed(speed_msg) => handle_speed_message(commands, players, speed_msg),
        ServerMessage::Face(face_msg) => handle_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_shot_message(commands, meshes, materials, players, player_face_query, shot_msg)
        }
        ServerMessage::Update(update_msg) => handle_update_message(
            commands,
            meshes,
            materials,
            players,
            rtt,
            past_pos_vel,
            player_pos_query,
            camera_query,
            my_player_id,
            update_msg,
        ),
        ServerMessage::Hit(hit_msg) => handle_hit_message(commands, players, camera_query, my_player_id, hit_msg),
        ServerMessage::Echo(echo_msg) => handle_echo_message(rtt, rtt_measurements, echo_msg),
    }
}

fn handle_login_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &mut ResMut<PlayerMap>,
    msg: &SLogin,
) {
    debug!("{:?} logged in", msg.id);
    if players.0.contains_key(&msg.id) {
        return;
    }

    let entity = spawn_player(
        commands,
        meshes,
        materials,
        msg.id.0,
        &msg.player.pos,
        msg.player.speed.to_velocity(),
        msg.player.face_dir,
        false,
    );
    players.0.insert(msg.id, PlayerInfo { entity, hits: 0 });
}

fn handle_logoff_message(commands: &mut Commands, players: &mut ResMut<PlayerMap>, msg: &SLogoff) {
    debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
    if let Some(player) = players.0.remove(&msg.id) {
        commands.entity(player.entity).despawn();
    }
}

fn handle_speed_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: &SSpeed) {
    trace!("{:?} speed: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(msg.speed.to_velocity());
    } else {
        warn!("received speed for non-existent player {:?}", msg.id);
    }
}

fn handle_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: &SFace) {
    trace!("{:?} face direction: {}", msg.id, msg.dir);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
    } else {
        warn!("received face direction for non-existent player {:?}", msg.id);
    }
}

fn handle_shot_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &ResMut<PlayerMap>,
    player_face_query: &Query<(&Position, &FaceDirection), With<PlayerId>>,
    msg: &SShot,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));
        spawn_projectile_for_player(commands, meshes, materials, player_face_query, player.entity);
    } else {
        warn!("received shot from non-existent player {:?}", msg.id);
    }
}

fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    past_pos_vel: &mut ResMut<PastPosVel>,
    player_pos_query: &Query<&Position, With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: &SUpdate,
) {
    // Track which players the server knows about in this snapshot
    let update_ids: HashSet<PlayerId> = msg.players.iter().map(|(id, _)| *id).collect();

    // Spawn any players that appear in the update but are missing locally
    for (id, player) in &msg.players {
        if players.0.contains_key(id) {
            continue;
        }

        let is_local = *id == my_player_id;
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
            past_pos_vel.pos = player.pos;
            past_pos_vel.vel = player.speed.to_velocity();
            past_pos_vel.timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();

            if let Ok(camera_entity) = camera_query.single() {
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

    // Despawn players no longer present in the authoritative snapshot
    let stale_ids: Vec<PlayerId> = players
        .0
        .keys()
        .filter(|id| !update_ids.contains(id))
        .copied()
        .collect();

    for id in stale_ids {
        if let Some(player) = players.0.remove(&id) {
            warn!("despawning player {:?} from Update", id);
            commands.entity(player.entity).despawn();
        }
    }

    // Apply the server’s latest transform/velocity, logging desync for the local player
    for (id, server_player) in &msg.players {
        if let Some(client_player) = players.0.get_mut(id) {
            if *id == my_player_id {
                if let Ok(client_pos) = player_pos_query.get(client_player.entity) {
                    log_local_desync(client_pos, server_player, &**past_pos_vel, &**rtt);
                }
            }

            commands
                .entity(client_player.entity)
                .insert((server_player.pos, server_player.speed.to_velocity()));
            client_player.hits = server_player.hits;
        }
    }
}

fn log_local_desync(client_pos: &Position, server_player: &Player, past_pos_vel: &PastPosVel, rtt: &RoundTripTime) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
    let elapsed_since_past = (now - past_pos_vel.timestamp) as f32;
    let past_pred = Position {
        x: past_pos_vel.pos.x + past_pos_vel.vel.x * elapsed_since_past,
        y: 0.0,
        z: past_pos_vel.pos.z + past_pos_vel.vel.z * elapsed_since_past,
    };

    let server_speed = server_player.speed.to_velocity();
    let half_rtt = (rtt.rtt / 2.0) as f32;
    let server_pred = Position {
        x: server_player.pos.x + server_speed.x * half_rtt,
        y: 0.0,
        z: server_player.pos.z + server_speed.z * half_rtt,
    };

    debug!(
        "client_z={:.2} server_offset={:+.2} server_pred_offset={:+.2} past_pred_offset={:+.2} {:?}",
        client_pos.z,
        server_player.pos.z - client_pos.z,
        server_pred.z - client_pos.z,
        past_pred.z - client_pos.z,
        server_player.speed.speed_level,
    );
}

fn handle_hit_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: &SHit,
) {
    debug!("player {:?} was hit", msg.id);
    if msg.id == my_player_id {
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                intensity: 3.0,
                dir_x: msg.hit_dir_x,
                dir_z: msg.hit_dir_z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
    } else if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(CuboidShake {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            intensity: 0.3,
            dir_x: msg.hit_dir_x,
            dir_z: msg.hit_dir_z,
            offset_x: 0.0,
            offset_z: 0.0,
        });
    }
}

fn handle_echo_message(rtt: &mut ResMut<RoundTripTime>, measurements: &mut VecDeque<f64>, msg: &SEcho) {
    if rtt.pending_timestamp == 0.0 || msg.timestamp != rtt.pending_timestamp {
        return;
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
    let measured_rtt = now - rtt.pending_timestamp;
    rtt.pending_timestamp = 0.0;

    measurements.push_back(measured_rtt);
    if measurements.len() > 10 {
        measurements.pop_front();
    }

    let sum: f64 = measurements.iter().sum();
    rtt.rtt = sum / measurements.len() as f64;
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
