use bevy::prelude::*;
use std::{collections::HashSet, time::Duration};

use super::{
    effects::{CameraShake, CuboidShake},
    movement::ServerReconciliation,
};
use crate::{
    constants::ECHO_INTERVAL,
    net::{ClientToServer, ServerToClient},
    resources::{
        ClientToServerChannel, ItemInfo, ItemMap, MyPlayerId, PlayerInfo, PlayerMap, RoundTripTime,
        ServerToClientChannel, WallConfig,
    },
    spawning::{spawn_item, spawn_player},
};
use common::constants::SPEED_POWER_UP_MULTIPLIER;
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
    mut item_map: ResMut<ItemMap>,
    mut rtt: ResMut<RoundTripTime>,
    player_query: Query<&Position, With<PlayerId>>,
    speed_query: Query<&Speed>,
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
                        message,
                        my_id.0,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut player_map,
                        &mut item_map,
                        &mut rtt,
                        &player_query,
                        &speed_query,
                        &player_face_query,
                        &camera_query,
                        &time,
                    );
                } else {
                    process_message_not_logged_in(message, &mut commands);
                }
            }
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn process_message_not_logged_in(msg: ServerMessage, commands: &mut Commands) {
    if let ServerMessage::Init(init_msg) = msg {
        debug!("received Init: my_id={:?}", init_msg.id);

        // Store player ID as resource
        commands.insert_resource(MyPlayerId(init_msg.id));

        // Store walls configuration
        commands.insert_resource(WallConfig {
            walls: init_msg.walls,
        });

        // Note: We don't spawn anything here. The first SUpdate will contain
        // all players including ourselves and will trigger spawning via the
        // Update message handler.
    }
}

fn process_message_logged_in(
    msg: ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &mut ResMut<RoundTripTime>,
    player_query: &Query<&Position, With<PlayerId>>,
    speed_query: &Query<&Speed>,
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
            items,
            rtt,
            player_query,
            camera_query,
            my_player_id,
            update_msg,
        ),
        ServerMessage::Hit(hit_msg) => handle_hit_message(commands, players, camera_query, my_player_id, hit_msg),
        ServerMessage::PowerUp(power_up_msg) => handle_power_up_message(commands, players, speed_query, power_up_msg),
        ServerMessage::Echo(echo_msg) => handle_echo_message(time, rtt, echo_msg),
    }
}

fn handle_login_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    players: &mut ResMut<PlayerMap>,
    msg: SLogin,
) {
    debug!("{:?} logged in", msg.id);
    if players.0.contains_key(&msg.id) {
        return;
    }

    let mut velocity = msg.player.speed.to_velocity();
    if msg.player.speed_power_up {
        velocity.x *= SPEED_POWER_UP_MULTIPLIER;
        velocity.z *= SPEED_POWER_UP_MULTIPLIER;
    }
    let entity = spawn_player(
        commands,
        meshes,
        materials,
        images,
        msg.id.0,
        &msg.player.name,
        &msg.player.pos,
        velocity,
        msg.player.face_dir,
        false,
    );
    players.0.insert(
        msg.id,
        PlayerInfo {
            entity,
            hits: 0,
            name: msg.player.name,
            speed_power_up: msg.player.speed_power_up,
            multi_shot_power_up: msg.player.multi_shot_power_up,
        },
    );
}

fn handle_logoff_message(commands: &mut Commands, players: &mut ResMut<PlayerMap>, msg: SLogoff) {
    debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
    if let Some(player) = players.0.remove(&msg.id) {
        commands.entity(player.entity).despawn();
    }
}

fn handle_speed_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: SSpeed) {
    trace!("{:?} speed: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        let mut velocity = msg.speed.to_velocity();
        if player.speed_power_up {
            velocity.x *= SPEED_POWER_UP_MULTIPLIER;
            velocity.z *= SPEED_POWER_UP_MULTIPLIER;
        }
        commands.entity(player.entity).insert(velocity);
    }
}

fn handle_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: SFace) {
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
    msg: SShot,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));
        
        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((pos, _)) = player_face_query.get(player.entity) {
            crate::spawning::spawn_projectiles_local(
                commands,
                meshes,
                materials,
                pos,
                msg.face_dir,
                player.multi_shot_power_up,
            );
        }
    }
}

fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &ResMut<RoundTripTime>,
    player_query: &Query<&Position, With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: SUpdate,
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
        let mut velocity = player.speed.to_velocity();
        if player.speed_power_up {
            velocity.x *= SPEED_POWER_UP_MULTIPLIER;
            velocity.z *= SPEED_POWER_UP_MULTIPLIER;
        }
        let entity = spawn_player(
            commands,
            meshes,
            materials,
            images,
            id.0,
            &player.name,
            &player.pos,
            velocity,
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
                speed_power_up: player.speed_power_up,
                multi_shot_power_up: player.multi_shot_power_up,
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

    // Update existing players with server state
    for (id, server_player) in msg.players {
        if let Some(client_player) = players.0.get_mut(&id) {
            if let Ok(client_pos) = player_query.get(client_player.entity) {
                let mut server_vel = server_player.speed.to_velocity();
                if server_player.speed_power_up {
                    server_vel.x *= SPEED_POWER_UP_MULTIPLIER;
                    server_vel.z *= SPEED_POWER_UP_MULTIPLIER;
                }
                commands.entity(client_player.entity).insert(ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: server_player.pos,
                    server_vel,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                });
            }

            client_player.hits = server_player.hits;
            client_player.speed_power_up = server_player.speed_power_up;
            client_player.multi_shot_power_up = server_player.multi_shot_power_up;
        }
    }

    // Handle items - track which ones server has by ID
    let server_item_ids: HashSet<ItemId> = msg.items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the update but are missing locally
    for (item_id, item) in &msg.items {
        if items.0.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(commands, meshes, materials, *item_id, item.item_type, &item.pos);
        items.0.insert(*item_id, ItemInfo { entity });
    }

    // Despawn items no longer present in the authoritative snapshot
    let stale_item_ids: Vec<ItemId> = items
        .0
        .keys()
        .filter(|id| !server_item_ids.contains(id))
        .copied()
        .collect();

    for item_id in stale_item_ids {
        if let Some(item_info) = items.0.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }
    }
}

fn handle_hit_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    msg: SHit,
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

fn handle_echo_message(time: &Time, rtt: &mut ResMut<RoundTripTime>, msg: SEcho) {
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

fn handle_power_up_message(commands: &mut Commands, players: &mut ResMut<PlayerMap>, speeds: &Query<&Speed>, msg: SPowerUp) {
    if let Some(player_info) = players.0.get_mut(&msg.id) {
        let old_speed_power_up = player_info.speed_power_up;
        player_info.speed_power_up = msg.speed_power_up;
        player_info.multi_shot_power_up = msg.multi_shot_power_up;
        
        // If speed power-up status changed, recalculate velocity
        if old_speed_power_up != msg.speed_power_up {
            if let Ok(speed) = speeds.get(player_info.entity) {
                let mut velocity = speed.to_velocity();
                if msg.speed_power_up {
                    velocity.x *= SPEED_POWER_UP_MULTIPLIER;
                    velocity.z *= SPEED_POWER_UP_MULTIPLIER;
                }
                commands.entity(player_info.entity).insert(velocity);
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
        let now = time.elapsed();
        rtt.pending_sent_at = now;
        #[allow(clippy::cast_possible_truncation)]
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Echo(CEcho {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}
