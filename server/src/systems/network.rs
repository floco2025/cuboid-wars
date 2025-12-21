use bevy::prelude::*;
use rand::prelude::*;

use crate::{
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, GridConfig, ItemMap, PlayerInfo, PlayerMap, SentryMap},
};
use common::protocol::MapLayout;
use common::{
    collision::projectiles::Projectile,
    constants::*,
    markers::{ItemMarker, PlayerMarker, ProjectileMarker, SentryMarker},
    protocol::*,
    spawning::calculate_projectile_spawns,
};

// ============================================================================
// Helper Functions
// ============================================================================

// Broadcast `message` to every logged-in player except `skip`.
pub fn broadcast_to_others(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in &players.0 {
        if *other_id != skip && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

// Broadcast `message` to every logged-in player.
pub fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.0.values() {
        if player_info.logged_in {
            let _ = player_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

fn snapshot_logged_in_players(
    players: &PlayerMap,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
) -> Vec<(PlayerId, Player)> {
    players
        .0
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let (pos, speed, face_dir) = player_data.get(info.entity).ok()?;
            Some((
                *player_id,
                Player {
                    name: info.name.clone(),
                    pos: *pos,
                    speed: *speed,
                    face_dir: face_dir.0,
                    hits: info.hits,
                    speed_power_up: ALWAYS_SPEED || info.speed_power_up_timer > 0.0,
                    multi_shot_power_up: ALWAYS_MULTI_SHOT || info.multi_shot_power_up_timer > 0.0,
                    phasing_power_up: ALWAYS_PHASING || info.phasing_power_up_timer > 0.0,
                    sentry_hunt_power_up: ALWAYS_SENTRY_HUNT || info.sentry_hunt_power_up_timer > 0.0,
                    stunned: info.stun_timer > 0.0,
                },
            ))
        })
        .collect()
}

// Build the authoritative item list that gets replicated to clients.
fn collect_items(items: &ItemMap, item_positions: &Query<&Position, With<ItemMarker>>) -> Vec<(ItemId, Item)> {
    items
        .0
        .iter()
        .filter(|(_, info)| {
            // Filter out cookies that are currently respawning (spawn_time > 0)
            info.item_type != ItemType::Cookie || info.spawn_time == 0.0
        })
        .map(|(id, info)| {
            let pos_component = item_positions.get(info.entity).expect("Item entity missing Position");
            (
                *id,
                Item {
                    item_type: info.item_type,
                    pos: *pos_component,
                },
            )
        })
        .collect()
}

// Build the authoritative sentry list that gets replicated to clients.
fn collect_sentries(
    sentries: &SentryMap,
    sentry_data: &Query<(&Position, &Velocity, &FaceDirection), With<SentryMarker>>,
) -> Vec<(SentryId, Sentry)> {
    sentries
        .0
        .iter()
        .map(|(id, info)| {
            let (pos_component, vel_component, face_dir_component) =
                sentry_data.get(info.entity).expect("Sentry entity missing components");
            (
                *id,
                Sentry {
                    pos: *pos_component,
                    vel: *vel_component,
                    face_dir: face_dir_component.0,
                },
            )
        })
        .collect()
}

// Generate a spawn position in a random grid cell without a ramp,
// spawning in the inner 50% of the cell to avoid walls.
fn generate_player_spawn_position(
    grid_config: &GridConfig,
    players: &PlayerMap,
    sentries: &SentryMap,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    sentry_data: &Query<(&Position, &Velocity, &FaceDirection), With<SentryMarker>>,
) -> Position {
    use common::constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE};

    let mut rng = rand::rng();
    let grid_rows = grid_config.grid.len() as i32;
    let grid_cols = grid_config.grid[0].len() as i32;
    let max_attempts = 100;
    const MIN_DISTANCE: f32 = 10.0; // Minimum distance from other entities

    // Collect all cells without ramps
    let mut valid_cells = Vec::new();
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !grid_config.grid[row as usize][col as usize].has_ramp {
                valid_cells.push((row, col));
            }
        }
    }

    if valid_cells.is_empty() {
        warn!("no valid spawn cells found (all have ramps), spawning at center");
        return Position::default();
    }

    for _ in 0..max_attempts {
        // Pick a random valid cell
        let &(row, col) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");

        // Calculate cell center in world coordinates
        let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

        // Spawn in inner 50% of the cell (25% margin from each edge)
        let spawn_range = GRID_SIZE * 0.5 / 2.0; // 50% of cell size / 2 for radius

        let pos = Position {
            x: cell_center_x + rng.random_range(-spawn_range..=spawn_range),
            y: 0.0,
            z: cell_center_z + rng.random_range(-spawn_range..=spawn_range),
        };

        // Check if position is too close to any existing player
        let too_close_to_player = players
            .0
            .values()
            .filter(|p| p.logged_in)
            .filter_map(|p| player_data.get(p.entity).ok())
            .any(|(p_pos, _, _)| {
                let dx = pos.x - p_pos.x;
                let dz = pos.z - p_pos.z;
                dx.mul_add(dx, dz * dz) < MIN_DISTANCE * MIN_DISTANCE
            });

        // Check if position is too close to any sentry
        let too_close_to_sentry =
            sentries
                .0
                .values()
                .filter_map(|s| sentry_data.get(s.entity).ok())
                .any(|(s_pos, _, _)| {
                    let dx = pos.x - s_pos.x;
                    let dz = pos.z - s_pos.z;
                    dx.mul_add(dx, dz * dz) < MIN_DISTANCE * MIN_DISTANCE
                });

        if !too_close_to_player && !too_close_to_sentry {
            return pos;
        }
    }

    // Fallback: return center if we somehow failed
    warn!(
        "Could not generate spawn position after {} attempts, spawning at center",
        max_attempts
    );
    Position::default()
}

// ============================================================================
// Accept Connections System
// ============================================================================

// Drain newly accepted connections into ECS entities and tracking state.
pub fn network_accept_connections_system(
    mut commands: Commands,
    mut from_accept: ResMut<FromAcceptChannel>,
    mut players: ResMut<PlayerMap>,
) {
    while let Ok((id, to_client)) = from_accept.try_recv() {
        debug!("{:?} connected", id);
        let entity = commands.spawn((PlayerMarker, id)).id();
        players.0.insert(
            id,
            PlayerInfo {
                entity,
                logged_in: false,
                channel: to_client,
                hits: 0,
                name: String::new(),
                speed_power_up_timer: 0.0,
                multi_shot_power_up_timer: 0.0,
                phasing_power_up_timer: 0.0,
                sentry_hunt_power_up_timer: 0.0,
                stun_timer: 0.0,
                last_shot_time: f32::NEG_INFINITY,
            },
        );
    }
}

// ============================================================================
// Client Event Processing System
// ============================================================================

// NOTE: Must run after accept_connections_system with apply_deferred in between, otherwise entities
// for the messages might not be spawned yet.
pub fn network_client_message_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    grid_config: Res<GridConfig>,
    items: Res<ItemMap>,
    sentries: Res<SentryMap>,
    player_data: Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    sentry_data: Query<(&Position, &Velocity, &FaceDirection), With<SentryMarker>>,
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
                    broadcast_to_others(&players, id, ServerMessage::Logoff(SLogoff { id, graceful: false }));
                }
            }
            ClientToServer::Message(message) => {
                let is_logged_in = player_info.logged_in;
                if is_logged_in {
                    process_message_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &time,
                        &player_data,
                        &map_layout,
                    );
                } else {
                    process_message_not_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &map_layout,
                        &grid_config,
                        &items,
                        &sentries,
                        &player_data,
                        &item_positions,
                        &sentry_data,
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
    players: &mut ResMut<PlayerMap>,
    map_layout: &Res<MapLayout>,
    grid_config: &Res<GridConfig>,
    items: &Res<ItemMap>,
    sentries: &Res<SentryMap>,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: &Query<&Position, With<ItemMarker>>,
    sentry_data: &Query<(&Position, &Velocity, &FaceDirection), With<SentryMarker>>,
) {
    match msg {
        ClientMessage::Login(login) => {
            debug!("{:?} logged in", id);

            let (channel, hits, name) = {
                let player_info = players
                    .0
                    .get_mut(&id)
                    .expect("process_message_not_logged_in called for unknown player");
                let channel = player_info.channel.clone();
                player_info.logged_in = true;

                // Determine player name: use provided name or default to the player id
                player_info.name = if login.name.is_empty() {
                    format!("Player {}", id.0)
                } else {
                    login.name
                };

                (channel, player_info.hits, player_info.name.clone())
            };

            // Send Init to the connecting player (their ID and grid config)
            let init_msg = ServerMessage::Init(SInit {
                id,
                map_layout: (*map_layout).clone(),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let pos = generate_player_spawn_position(grid_config, players, sentries, player_data, sentry_data);

            // Calculate initial facing direction toward center
            let face_dir = (-pos.x).atan2(-pos.z);

            // Initial speed for the new player
            let speed = Speed {
                speed_level: SpeedLevel::Idle,
                // move_dir: 0.0,
                move_dir: std::f32::consts::PI, // Same as face_dir - facing toward origin
            };

            // Construct player data
            let player = Player {
                name,
                pos,
                speed,
                face_dir,
                hits,
                speed_power_up: false,
                multi_shot_power_up: false,
                phasing_power_up: false,
                sentry_hunt_power_up: false,
                stunned: false,
            };

            // Construct the initial Update for the new player
            let mut all_players = snapshot_logged_in_players(players, player_data)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial update
            let all_items = collect_items(items, item_positions);

            // Collect all sentries for the initial update
            let all_sentries = collect_sentries(sentries, sentry_data);

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate {
                seq: 0,
                players: all_players,
                items: all_items,
                sentries: all_sentries,
            });
            channel.send(ServerToClient::Send(update_msg)).ok();

            // Now update entity: add Position + Speed + FaceDirection
            commands.entity(entity).insert((pos, speed, FaceDirection(face_dir)));

            // Broadcast Login to all other logged-in players
            let login_msg = SLogin { id, player };
            broadcast_to_others(players, id, ServerMessage::Login(login_msg));
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
// Movement Handlers
// ============================================================================

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

fn handle_face_direction(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CFace, players: &PlayerMap) {
    // Update the player's face direction
    commands.entity(entity).insert(FaceDirection(msg.dir));

    broadcast_to_others(players, id, ServerMessage::Face(SFace { id, dir: msg.dir }));
}

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

// ============================================================================
// Broadcast System
// ============================================================================

// Broadcast authoritative game state in regular time intervals
pub fn network_broadcast_state_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seq: Local<u32>,
    players: Res<PlayerMap>,
    items: Res<ItemMap>,
    sentries: Res<SentryMap>,
    player_data: Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    sentry_data: Query<(&Position, &Velocity, &FaceDirection), With<SentryMarker>>,
) {
    *timer += time.delta_secs();
    if *timer < UPDATE_BROADCAST_INTERVAL {
        return;
    }
    *timer = 0.0;

    // Increment sequence number
    *seq = seq.wrapping_add(1);

    if players.0.values().all(|info| !info.logged_in) {
        return; // Nothing to broadcast yet
    }

    // Collect all logged-in players
    let all_players = snapshot_logged_in_players(&players, &player_data);

    // Collect all items
    let all_items = collect_items(&items, &item_positions);

    // Collect all sentries
    let all_sentries = collect_sentries(&sentries, &sentry_data);

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate {
        seq: *seq,
        players: all_players,
        items: all_items,
        sentries: all_sentries,
    });
    //trace!("broadcasting update: {:?}", msg);
    broadcast_to_all(&players, msg);
}
