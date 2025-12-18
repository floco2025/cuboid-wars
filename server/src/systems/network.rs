use bevy::{ecs::system::SystemParam, prelude::*};
use rand::Rng;

use crate::{
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, GhostMap, GridConfig, ItemMap, PlayerInfo, PlayerMap},
};
use common::{
    collision::{players::overlap_player_vs_wall, projectile::Projectile},
    constants::*,
    markers::{PlayerMarker, ProjectileMarker},
    protocol::*,
    ramps::is_on_ramp,
    spawning::calculate_projectile_spawns,
};

// ============================================================================
// SystemParam Bundles
// ============================================================================

// Groups commonly used queries for network message processing
#[derive(SystemParam)]
pub struct NetworkEntityQueries<'w, 's> {
    pub positions: Query<'w, 's, &'static Position>,
    pub speeds: Query<'w, 's, &'static Speed>,
    pub face_dirs: Query<'w, 's, &'static FaceDirection>,
    pub velocities: Query<'w, 's, &'static Velocity>,
}

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

fn snapshot_logged_in_players(players: &PlayerMap, queries: &NetworkEntityQueries) -> Vec<(PlayerId, Player)> {
    players
        .0
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let pos = queries.positions.get(info.entity).ok()?;
            let speed = queries.speeds.get(info.entity).ok()?;
            let face_dir = queries.face_dirs.get(info.entity).ok()?;
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
                    reflect_power_up: ALWAYS_REFLECT || info.reflect_power_up_timer > 0.0,
                    phasing_power_up: ALWAYS_PHASING || info.phasing_power_up_timer > 0.0,
                    ghost_hunt_power_up: ALWAYS_GHOST_HUNT || info.ghost_hunt_power_up_timer > 0.0,
                    stunned: info.stun_timer > 0.0,
                },
            ))
        })
        .collect()
}

// Build the authoritative item list that gets replicated to clients.
fn collect_items(items: &ItemMap, positions: &Query<&Position>) -> Vec<(ItemId, Item)> {
    items
        .0
        .iter()
        .filter(|(_, info)| {
            // Filter out cookies that are currently respawning (spawn_time > 0)
            info.item_type != ItemType::Cookie || info.spawn_time == 0.0
        })
        .map(|(id, info)| {
            let pos_component = positions.get(info.entity).expect("Item entity missing Position");
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

// Build the authoritative ghost list that gets replicated to clients.
fn collect_ghosts(ghosts: &GhostMap, queries: &NetworkEntityQueries) -> Vec<(GhostId, Ghost)> {
    ghosts
        .0
        .iter()
        .map(|(id, info)| {
            let pos_component = queries
                .positions
                .get(info.entity)
                .expect("Ghost entity missing Position");
            let vel_component = queries
                .velocities
                .get(info.entity)
                .expect("Ghost entity missing Velocity");
            (
                *id,
                Ghost {
                    pos: *pos_component,
                    vel: *vel_component,
                },
            )
        })
        .collect()
}

// Try to find a spawn point that does not intersect any generated wall or ramp.
fn generate_spawn_position(grid_config: &GridConfig) -> Position {
    let mut rng = rand::rng();
    let max_attempts = 100;

    for _ in 0..max_attempts {
        let pos = Position {
            x: rng.random_range(-FIELD_WIDTH / 2.0..=FIELD_WIDTH / 2.0),
            y: 0.0,
            z: rng.random_range(-FIELD_DEPTH / 2.0..=FIELD_DEPTH / 2.0),
        };

        // Check if position intersects with any wall
        let intersects = grid_config
            .all_walls
            .iter()
            .any(|wall| overlap_player_vs_wall(&pos, wall));

        // Check if position is on a ramp
        let on_ramp = is_on_ramp(&grid_config.ramps, pos.x, pos.z);

        if !intersects && !on_ramp {
            return pos;
        }
    }

    // Fallback: return center if we couldn't find a valid position
    warn!(
        "could not find spawn position without wall collision after {} attempts, spawning at center",
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
                reflect_power_up_timer: 0.0,
                phasing_power_up_timer: 0.0,
                ghost_hunt_power_up_timer: 0.0,
                stun_timer: 0.0,
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
    grid_config: Res<GridConfig>,
    items: Res<ItemMap>,
    ghosts: Res<GhostMap>,
    queries: NetworkEntityQueries,
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
                        &players,
                        &queries,
                        &grid_config,
                    );
                } else {
                    process_message_not_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &queries,
                        &mut players,
                        &grid_config,
                        &items,
                        &ghosts,
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
    queries: &NetworkEntityQueries,
    players: &mut ResMut<PlayerMap>,
    grid_config: &Res<GridConfig>,
    items: &Res<ItemMap>,
    ghosts: &Res<GhostMap>,
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

            // Send Init to the connecting player (their ID, walls, and roofs)
            let init_msg = ServerMessage::Init(SInit {
                id,
                boundary_walls: grid_config.boundary_walls.clone(),
                interior_walls: grid_config.interior_walls.clone(),
                roofs: grid_config.roofs.clone(),
                ramps: grid_config.ramps.clone(),
                roof_edge_walls: grid_config.roof_edge_walls.clone(),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let pos = generate_spawn_position(grid_config);

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
                reflect_power_up: false,
                phasing_power_up: false,
                ghost_hunt_power_up: false,
                stunned: false,
            };

            // Construct the initial Update for the new player
            let mut all_players = snapshot_logged_in_players(players, queries)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial update
            let all_items = collect_items(items, &queries.positions);

            // Collect all ghosts for the initial update
            let all_ghosts = collect_ghosts(ghosts, queries);

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate {
                seq: 0,
                players: all_players,
                items: all_items,
                ghosts: all_ghosts,
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
    players: &PlayerMap,
    queries: &NetworkEntityQueries,
    grid_config: &GridConfig,
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
            handle_speed(commands, entity, id, msg, players, &queries.positions);
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_direction(commands, entity, id, msg, players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot(commands, entity, id, msg, players, &queries.positions, grid_config);
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
    positions: &Query<&Position>,
) {
    // Update the player's speed
    commands.entity(entity).insert(msg.speed);

    // Get current position for reconciliation
    if let Ok(pos) = positions.get(entity) {
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
    players: &PlayerMap,
    positions: &Query<&Position>,
    grid_config: &GridConfig,
) {
    // Update the shooter's face direction to exact facing direction
    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    // Spawn projectile(s) on server for hit detection
    if let Ok(pos) = positions.get(entity) {
        // Check if player has reflect power-up
        let has_reflect = ALWAYS_REFLECT
            || players.0.get(&id).is_some_and(|info| info.reflect_power_up_timer > 0.0);

        // Check if player has multi-shot power-up
        let has_multi_shot = ALWAYS_MULTI_SHOT
            || players
                .0
                .get(&id)
                .is_some_and(|info| info.multi_shot_power_up_timer > 0.0);

        // Calculate valid projectile spawn positions (all_walls excludes roof-edge guards)
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_dir,
            msg.face_pitch,
            has_multi_shot,
            has_reflect,
            &grid_config.all_walls,
            &grid_config.ramps,
        );

        // Spawn each projectile
        for spawn_info in spawns {
            let projectile =
                Projectile::new(spawn_info.direction_yaw, spawn_info.direction_pitch, spawn_info.reflects);

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
    queries: NetworkEntityQueries,
    players: Res<PlayerMap>,
    items: Res<ItemMap>,
    ghosts: Res<GhostMap>,
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
    let all_players = snapshot_logged_in_players(&players, &queries);

    // Collect all items
    let all_items = collect_items(&items, &queries.positions);

    // Collect all ghosts
    let all_ghosts = collect_ghosts(&ghosts, &queries);

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate {
        seq: *seq,
        players: all_players,
        items: all_items,
        ghosts: all_ghosts,
    });
    //trace!("broadcasting update: {:?}", msg);
    broadcast_to_all(&players, msg);
}
