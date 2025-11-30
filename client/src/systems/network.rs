use bevy::prelude::*;
use std::{collections::HashSet, time::Duration};

use super::{
    effects::{CameraShake, CuboidShake},
    movement::ServerSnapshot,
};
use crate::{
    constants::ECHO_INTERVAL,
    net::{ClientToServer, ServerToClient},
    resources::{
        ClientToServerChannel, MyPlayerId, PlayerInfo, PlayerMap, RoundTripTime, ServerToClientChannel, WallConfig,
    },
    spawning::{spawn_player, spawn_projectile_for_player},
};
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
    mut images: ResMut<Assets<Image>>,
    mut player_map: ResMut<PlayerMap>,
    mut rtt: ResMut<RoundTripTime>,
    player_query: Query<(&Position, &Velocity), With<PlayerId>>,
    player_face_query: Query<(&Position, &FaceDirection), With<PlayerId>>,
    camera_query: Query<Entity, With<Camera3d>>,
    my_player_id: Option<Res<MyPlayerId>>,
    time: Res<Time>,
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
                        &message,
                        my_id.0,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut player_map,
                        &mut rtt,
                        &player_query,
                        &player_face_query,
                        &camera_query,
                        &time,
                    );
                } else {
                    process_message_not_logged_in(&message, &mut commands);
                }
            }
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn process_message_not_logged_in(msg: &ServerMessage, commands: &mut Commands) {
    if let ServerMessage::Init(init_msg) = msg {
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
}

fn process_message_logged_in(
    msg: &ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &mut ResMut<RoundTripTime>,
    player_query: &Query<(&Position, &Velocity), With<PlayerId>>,
    player_face_query: &Query<(&Position, &FaceDirection), With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    time: &Res<Time>,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(login) => handle_login_message(commands, meshes, materials, images, players, login),
        ServerMessage::Logoff(logoff) => handle_logoff_message(commands, players, logoff),
        ServerMessage::Speed(speed_msg) => handle_speed_message(commands, players, speed_msg),
        ServerMessage::Face(face_msg) => handle_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_shot_message(commands, meshes, materials, players, player_face_query, shot_msg);
        }
        ServerMessage::Update(update_msg) => handle_update_message(
            commands,
            meshes,
            materials,
            images,
            players,
            player_query,
            camera_query,
            my_player_id,
            update_msg,
            time,
        ),
        ServerMessage::Hit(hit_msg) => handle_hit_message(commands, players, camera_query, my_player_id, hit_msg),
        ServerMessage::Echo(echo_msg) => handle_echo_message(time, rtt, echo_msg),
    }
}

fn handle_login_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
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
        images,
        msg.id.0,
        &msg.player.name,
        &msg.player.pos,
        msg.player.speed.to_velocity(),
        msg.player.face_dir,
        false,
    );
    players.0.insert(
        msg.id,
        PlayerInfo {
            entity,
            hits: 0,
            name: msg.player.name.clone(),
        },
    );
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
    }
}

fn handle_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: &SFace) {
    trace!("{:?} face direction: {}", msg.id, msg.dir);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
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
    }
}

fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    players: &mut ResMut<PlayerMap>,
    player_query: &Query<(&Position, &Velocity), With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: &SUpdate,
    time: &Time,
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
            images,
            id.0,
            &player.name,
            &player.pos,
            player.speed.to_velocity(),
            player.face_dir,
            is_local,
        );

        if is_local && let Ok(camera_entity) = camera_query.single() {
            let camera_rotation = player.face_dir + std::f32::consts::PI;
            commands.entity(camera_entity).insert(
                Transform::from_xyz(player.pos.x, 2.5, player.pos.z + 3.0)
                    .with_rotation(Quat::from_rotation_y(camera_rotation)),
            );
        }

        players.0.insert(
            *id,
            PlayerInfo {
                entity,
                hits: player.hits,
                name: player.name.clone(),
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
            commands.entity(player.entity).despawn();
        }
    }

    // Apply the server's latest transform/velocity, logging desync for the local player
    let now = time.elapsed();

    for (id, server_player) in &msg.players {
        if let Some(client_player) = players.0.get_mut(id) {
            if let Ok((client_pos, client_vel)) = player_query.get(client_player.entity) {
                let server_vel = server_player.speed.to_velocity();

                commands.entity(client_player.entity).insert(ServerSnapshot {
                    client_pos: *client_pos,
                    client_vel: *client_vel,
                    server_pos: server_player.pos,
                    server_vel,
                    received_at: now,
                    timer: 0.0,
                });
            }

            client_player.hits = server_player.hits;
        }
    }
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

fn handle_echo_message(time: &Time, rtt: &mut ResMut<RoundTripTime>, msg: &SEcho) {
    if rtt.pending_sent_at == Duration::ZERO {
        return;
    }

    #[allow(clippy::cast_possible_truncation)]
    let expected_nanos = rtt.pending_sent_at.as_nanos() as u64;
    if msg.timestamp_nanos != expected_nanos {
        return;
    }

    let now = time.elapsed();
    let measured_rtt = now - rtt.pending_sent_at;
    rtt.pending_sent_at = Duration::ZERO;

    rtt.measurements.push_back(measured_rtt);
    if rtt.measurements.len() > 10 {
        rtt.measurements.pop_front();
    }

    let sum: Duration = rtt.measurements.iter().sum();
    #[allow(clippy::cast_possible_truncation)]
    {
        rtt.rtt = sum / rtt.measurements.len() as u32;
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
        let now = time.elapsed();
        rtt.pending_sent_at = now;
        #[allow(clippy::cast_possible_truncation)]
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Echo(CEcho {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}
