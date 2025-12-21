use bevy::{ecs::system::SystemParam, prelude::*};
use std::{collections::HashSet, time::Duration};

use super::players::{CameraShake, CuboidShake, PlayerMovement};
use crate::{
    constants::ECHO_INTERVAL,
    net::{ClientToServer, ServerToClient},
    resources::{
        ClientToServerChannel, ItemInfo, ItemMap, LastUpdateSeq, MyPlayerId, PlayerInfo, PlayerMap, RoundTripTime,
        SentryInfo, SentryMap, ServerToClientChannel,
    },
    spawning::{spawn_item, spawn_player, spawn_projectiles, spawn_sentry},
};
use common::{
    constants::POWER_UP_SPEED_MULTIPLIER,
    markers::{PlayerMarker, SentryMarker},
    protocol::*,
};

// ============================================================================
// Components
// ============================================================================

// Server's authoritative snapshot for this entity
#[derive(Component)]
pub struct ServerReconciliation {
    pub client_pos: Position,
    pub server_pos: Position,
    pub server_vel: Velocity,
    pub timer: f32,
    pub rtt: f32,
}

// System params to reduce parameter count
#[derive(SystemParam)]
pub struct AssetManagers<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub graphs: ResMut<'w, Assets<AnimationGraph>>,
}

// ============================================================================
// Network Message Processing
// ============================================================================

// System to process messages from the server
pub fn network_server_message_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    mut sentries: ResMut<SentryMap>,
    mut rtt: ResMut<RoundTripTime>,
    mut last_update_seq: ResMut<LastUpdateSeq>,
    mut assets: AssetManagers,
    player_data: Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    sentry_positions: Query<&Position, With<SentryMarker>>,
    cameras: Query<Entity, With<Camera3d>>,
    my_player_id: Option<Res<MyPlayerId>>,
    map_layout: Option<Res<MapLayout>>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
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
                        &mut players,
                        &mut items,
                        &mut sentries,
                        &mut rtt,
                        &mut last_update_seq,
                        &mut assets,
                        &player_data,
                        &sentry_positions,
                        &cameras,
                        &time,
                        &asset_server,
                        map_layout.as_deref(),
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

        // Store grid configuration
        commands.insert_resource(init_msg.map_layout);

        // Note: We don't spawn anything here. The first SUpdate will contain
        // all players including ourselves and will trigger spawning via the
        // Update message handler.
    }
}

fn process_message_logged_in(
    msg: ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &mut ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    assets: &mut AssetManagers,
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    sentry_positions: &Query<&Position, With<SentryMarker>>,
    cameras: &Query<Entity, With<Camera3d>>,
    time: &Res<Time>,
    asset_server: &Res<AssetServer>,
    map_layout: Option<&MapLayout>,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(login) => handle_login_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            asset_server,
            login,
        ),
        ServerMessage::Logoff(logoff) => handle_logoff_message(commands, players, logoff),
        ServerMessage::Speed(speed_msg) => {
            handle_speed_message(commands, players, player_data, rtt, speed_msg);
        }
        ServerMessage::Face(face_msg) => handle_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_shot_message(
                commands,
                &mut assets.meshes,
                &mut assets.materials,
                players,
                player_data,
                shot_msg,
                map_layout,
            );
        }
        ServerMessage::Update(update_msg) => handle_update_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            items,
            sentries,
            rtt,
            last_update_seq,
            player_data,
            sentry_positions,
            cameras,
            my_player_id,
            asset_server,
            update_msg,
        ),
        ServerMessage::Hit(hit_msg) => handle_hit_message(commands, players, cameras, my_player_id, hit_msg),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(
                commands,
                players,
                player_data,
                player_status_msg,
                my_player_id,
                asset_server,
            );
        }
        ServerMessage::Echo(echo_msg) => handle_echo_message(time, rtt, echo_msg),
        ServerMessage::Sentry(sentry_msg) => {
            handle_sentry_message(
                commands,
                &mut assets.meshes,
                &mut assets.materials,
                &mut assets.graphs,
                sentries,
                rtt,
                sentry_positions,
                sentry_msg,
                asset_server,
            );
        }
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_cookie_collected_message(commands, cookie_msg, asset_server);
        }
        ServerMessage::SentryHit(sentry_hit_msg) => {
            handle_sentry_hit_message(commands, sentry_hit_msg, asset_server);
        }
    }
}

fn handle_login_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    asset_server: &Res<AssetServer>,
    msg: SLogin,
) {
    debug!("{:?} logged in", msg.id);
    if players.0.contains_key(&msg.id) {
        return;
    }

    let mut velocity = msg.player.speed.to_velocity();
    if msg.player.speed_power_up {
        velocity.x *= POWER_UP_SPEED_MULTIPLIER;
        velocity.z *= POWER_UP_SPEED_MULTIPLIER;
    }
    let entity = spawn_player(
        commands,
        asset_server,
        meshes,
        materials,
        images,
        graphs,
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
            phasing_power_up: msg.player.phasing_power_up,
            sentry_hunt_power_up: msg.player.sentry_hunt_power_up,
            stunned: msg.player.stunned,
        },
    );
}

fn handle_logoff_message(commands: &mut Commands, players: &mut ResMut<PlayerMap>, msg: SLogoff) {
    debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
    if let Some(player) = players.0.remove(&msg.id) {
        commands.entity(player.entity).despawn();
    }
}

fn handle_speed_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    msg: SSpeed,
) {
    trace!("{:?} speed: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        let mut velocity = msg.speed.to_velocity();
        if player.speed_power_up {
            velocity.x *= POWER_UP_SPEED_MULTIPLIER;
            velocity.z *= POWER_UP_SPEED_MULTIPLIER;
        }

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                velocity, // Never the local player, so we can always insert velocity
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.pos,
                    server_vel: velocity,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            commands.entity(player.entity).insert(velocity);
        }
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
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    msg: SShot,
    map_layout: Option<&MapLayout>,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((_, _, player_facing)) = player_data.get(player.entity)
            && let Some(map_layout) = map_layout
        {
            spawn_projectiles(
                commands,
                meshes,
                materials,
                player_facing.position,
                msg.face_dir,
                msg.face_pitch,
                player.multi_shot_power_up,
                map_layout.lower_walls.as_slice(),
                map_layout.ramps.as_slice(),
                map_layout.roofs.as_slice(),
                msg.id,
            );
        }
    }
}

fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    sentry_query: &Query<&Position, With<SentryMarker>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    msg: SUpdate,
) {
    // Ignore outdated updates
    if msg.seq <= last_update_seq.0 {
        warn!(
            "Ignoring outdated SUpdate (seq: {}, last: {})",
            msg.seq, last_update_seq.0
        );
        return;
    }

    // Update the last received sequence number
    last_update_seq.0 = msg.seq;

    handle_players_update(
        commands,
        meshes,
        materials,
        images,
        graphs,
        players,
        rtt,
        player_data,
        camera_query,
        my_player_id,
        asset_server,
        &msg.players,
    );
    handle_items_update(commands, meshes, materials, items, asset_server, &msg.items);
    handle_sentrys_update(
        commands,
        meshes,
        materials,
        graphs,
        sentries,
        rtt,
        sentry_query,
        &msg.sentries,
        asset_server,
    );
}

fn handle_players_update(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    server_players: &[(PlayerId, Player)],
) {
    // Track which players the server knows about in this snapshot
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    // Spawn any players that appear in the update but are missing locally
    for (id, player) in server_players {
        if players.0.contains_key(id) {
            continue;
        }

        let is_local = *id == my_player_id;
        debug!("spawning player {:?} from Update (is_local: {})", id, is_local);
        let mut velocity = player.speed.to_velocity();
        if player.speed_power_up {
            velocity.x *= POWER_UP_SPEED_MULTIPLIER;
            velocity.z *= POWER_UP_SPEED_MULTIPLIER;
        }
        let entity = spawn_player(
            commands,
            asset_server,
            meshes,
            materials,
            images,
            graphs,
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
                phasing_power_up: player.phasing_power_up,
                sentry_hunt_power_up: player.sentry_hunt_power_up,
                stunned: player.stunned,
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
    for (id, server_player) in server_players {
        if let Some(client_player) = players.0.get_mut(id) {
            if let Ok((client_pos, _, _)) = player_data.get(client_player.entity) {
                let mut server_vel = server_player.speed.to_velocity();
                if server_player.speed_power_up {
                    server_vel.x *= POWER_UP_SPEED_MULTIPLIER;
                    server_vel.z *= POWER_UP_SPEED_MULTIPLIER;
                }

                // The local player's velocity is always authoritive, so don't overwrite from
                // server updates
                if *id != my_player_id {
                    commands.entity(client_player.entity).insert(server_vel);
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
            client_player.phasing_power_up = server_player.phasing_power_up;
        }
    }
}

fn handle_items_update(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    items: &mut ResMut<ItemMap>,
    asset_server: &Res<AssetServer>,
    server_items: &[(ItemId, Item)],
) {
    let server_item_ids: HashSet<ItemId> = server_items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the update but are missing locally
    for (item_id, item) in server_items {
        if items.0.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(
            commands,
            meshes,
            materials,
            asset_server,
            *item_id,
            item.item_type,
            &item.pos,
        );
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

fn handle_sentrys_update(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &ResMut<RoundTripTime>,
    sentry_query: &Query<&Position, With<SentryMarker>>,
    server_sentrys: &[(SentryId, Sentry)],
    asset_server: &Res<AssetServer>,
) {
    let server_sentry_ids: HashSet<SentryId> = server_sentrys.iter().map(|(id, _)| *id).collect();

    // Spawn any sentries that appear in the update but are missing locally
    for (sentry_id, server_sentry) in server_sentrys {
        if sentries.0.contains_key(sentry_id) {
            continue;
        }
        let entity = spawn_sentry(
            commands,
            meshes,
            materials,
            asset_server,
            graphs,
            *sentry_id,
            &server_sentry.pos,
            &server_sentry.vel,
            server_sentry.face_dir,
        );
        sentries.0.insert(*sentry_id, SentryInfo { entity });
    }

    // Despawn sentries no longer present in the authoritative snapshot
    let stale_sentry_ids: Vec<SentryId> = sentries
        .0
        .keys()
        .filter(|id| !server_sentry_ids.contains(id))
        .copied()
        .collect();

    for sentry_id in stale_sentry_ids {
        if let Some(sentry_info) = sentries.0.remove(&sentry_id) {
            commands.entity(sentry_info.entity).despawn();
        }
    }

    // Update existing sentries with server state (position and velocity)
    for (sentry_id, server_sentry) in server_sentrys {
        if let Some(client_sentry) = sentries.0.get(sentry_id) {
            // Check if we have a client position to track reconciliation
            if let Ok(client_pos) = sentry_query.get(client_sentry.entity) {
                commands.entity(client_sentry.entity).insert((
                    server_sentry.vel,
                    ServerReconciliation {
                        client_pos: *client_pos,
                        server_pos: server_sentry.pos,
                        server_vel: server_sentry.vel,
                        timer: 0.0,
                        rtt: rtt.rtt.as_secs_f32(),
                    },
                ));
            } else {
                // No client position yet, just set server state
                commands
                    .entity(client_sentry.entity)
                    .insert((server_sentry.pos, server_sentry.vel));
            }
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
    {
        rtt.rtt = sum / rtt.measurements.len() as u32;
    }
}

fn handle_player_status_message(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    player_data: &Query<(&Position, &Speed, PlayerMovement), With<PlayerMarker>>,
    msg: SPlayerStatus,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
) {
    if let Some(player_info) = players.0.get_mut(&msg.id) {
        // Play power-up sound effect only for the local player
        if msg.id == my_player_id {
            // Don't play power-up sound effect if this message is due to a stun change
            if player_info.stunned == msg.stunned {
                // Only play power-up sound effect if it wasn't a downgrade
                #[allow(clippy::nonminimal_bool)]
                if !(player_info.speed_power_up && !msg.speed_power_up
                    || player_info.multi_shot_power_up && !msg.multi_shot_power_up
                    || player_info.phasing_power_up && !msg.phasing_power_up)
                {
                    commands.spawn((
                        AudioPlayer::new(asset_server.load("sounds/player_powerup.wav")),
                        PlaybackSettings::DESPAWN,
                    ));
                }
            }
        }

        // If speed power-up status changed, recalculate velocity
        if player_info.speed_power_up != msg.speed_power_up
            && let Ok((_, speed, _)) = player_data.get(player_info.entity)
        {
            let mut velocity = speed.to_velocity();
            if msg.speed_power_up {
                velocity.x *= POWER_UP_SPEED_MULTIPLIER;
                velocity.z *= POWER_UP_SPEED_MULTIPLIER;
            }

            commands.entity(player_info.entity).insert(velocity);
        }

        player_info.speed_power_up = msg.speed_power_up;
        player_info.multi_shot_power_up = msg.multi_shot_power_up;
        player_info.phasing_power_up = msg.phasing_power_up;
        player_info.sentry_hunt_power_up = msg.sentry_hunt_power_up;
        player_info.stunned = msg.stunned;
    }
}

fn handle_sentry_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &ResMut<RoundTripTime>,
    sentry_query: &Query<&Position, With<SentryMarker>>,
    msg: SSentry,
    asset_server: &Res<AssetServer>,
) {
    if let Some(sentry_info) = sentries.0.get(&msg.id) {
        // Update existing sentry with reconciliation
        if let Ok(client_pos) = sentry_query.get(sentry_info.entity) {
            commands.entity(sentry_info.entity).insert((
                msg.sentry.vel,
                FaceDirection(msg.sentry.face_dir),
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.sentry.pos,
                    server_vel: msg.sentry.vel,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            // No client position yet, just set server state
            commands.entity(sentry_info.entity).insert((
                msg.sentry.pos,
                msg.sentry.vel,
                FaceDirection(msg.sentry.face_dir),
            ));
        }
    } else {
        // Spawn new sentry
        let entity = spawn_sentry(
            commands,
            meshes,
            materials,
            asset_server,
            graphs,
            msg.id,
            &msg.sentry.pos,
            &msg.sentry.vel,
            msg.sentry.face_dir,
        );
        sentries.0.insert(msg.id, SentryInfo { entity });
    }
}

fn handle_cookie_collected_message(commands: &mut Commands, _msg: SCookieCollected, asset_server: &AssetServer) {
    // Play sound - this message is only sent to the player who collected it
    commands.spawn((
        AudioPlayer::new(asset_server.load("sounds/player_cookie.ogg")),
        PlaybackSettings::DESPAWN,
    ));
}

fn handle_sentry_hit_message(commands: &mut Commands, _msg: SSentryHit, asset_server: &AssetServer) {
    // Play sound - this message is only sent to the player who was hit
    commands.spawn((
        AudioPlayer::new(asset_server.load("sounds/sentry_hits_player.wav")),
        PlaybackSettings::DESPAWN,
    ));
}

// ============================================================================
// Echo/Ping System
// ============================================================================

// System to send echo requests every ECHO_INTERVAL seconds
pub fn network_echo_system(
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
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Echo(CEcho {
            timestamp_nanos: now.as_nanos() as u64,
        })));
    }
}
