use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::*,
    net::{ClientToServer, ServerToClient},
    resources::{
        FromAcceptChannel, FromClientsChannel, GridConfig, ItemInfo, ItemMap, ItemSpawner, PlayerInfo, PlayerMap,
    },
};
use common::{
    collision::{
        check_ghost_wall_collision, check_player_item_collision, check_player_player_collision,
        check_player_wall_collision, check_projectile_player_hit,
    },
    constants::*,
    protocol::*,
    systems::Projectile,
};

// ============================================================================
// Helper Functions
// ============================================================================

// Generate a random spawn position that doesn't intersect with any walls
fn generate_spawn_position(grid_config: &GridConfig) -> Position {
    let mut rng = rand::rng();
    let max_attempts = 100;

    for _ in 0..max_attempts {
        let pos = Position {
            x: rng.random_range(-SPAWN_RANGE_X..=SPAWN_RANGE_X),
            y: 0.0,
            z: rng.random_range(-SPAWN_RANGE_Z..=SPAWN_RANGE_Z),
        };

        // Check if position intersects with any wall
        let intersects = grid_config
            .walls
            .iter()
            .any(|wall| check_player_wall_collision(&pos, wall));

        if !intersects {
            return pos;
        }
    }

    // Fallback: return center if we couldn't find a valid position
    warn!(
        "Could not find spawn position without wall collision after {} attempts, spawning at center",
        max_attempts
    );
    Position::default()
}

fn broadcast_to_logged_in(players: &PlayerMap, skip: PlayerId, message: ServerMessage) {
    for (other_id, other_info) in &players.0 {
        if *other_id != skip && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

fn broadcast_to_all(players: &PlayerMap, message: ServerMessage) {
    for player_info in players.0.values() {
        if player_info.logged_in {
            let _ = player_info.channel.send(ServerToClient::Send(message.clone()));
        }
    }
}

fn snapshot_logged_in_players(
    players: &PlayerMap,
    positions: &Query<&Position>,
    speeds: &Query<&Speed>,
    face_dirs: &Query<&FaceDirection>,
) -> Vec<(PlayerId, Player)> {
    players
        .0
        .iter()
        .filter_map(|(player_id, info)| {
            if !info.logged_in {
                return None;
            }
            let pos = positions.get(info.entity).ok()?;
            let speed = speeds.get(info.entity).ok()?;
            let face_dir = face_dirs.get(info.entity).ok()?;
            Some((
                *player_id,
                Player {
                    name: info.name.clone(),
                    pos: *pos,
                    speed: *speed,
                    face_dir: face_dir.0,
                    hits: info.hits,
                    speed_power_up: info.speed_power_up_timer > 0.0,
                    multi_shot_power_up: info.multi_shot_power_up_timer > 0.0,
                },
            ))
        })
        .collect()
}

// ============================================================================
// Accept Connections System
// ============================================================================

/// Drain newly accepted connections into ECS entities and tracking state.
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
                hits: 0,
                name: String::new(),
                speed_power_up_timer: 0.0,
                multi_shot_power_up_timer: 0.0,
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
    items: Res<ItemMap>,
    grid_config: Res<GridConfig>,
    ghosts: Res<crate::resources::GhostMap>,
    positions: Query<&Position>,
    speeds: Query<&Speed>,
    face_dirs: Query<&FaceDirection>,
    velocities: Query<&Velocity>,
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
                    broadcast_to_logged_in(&players, id, ServerMessage::Logoff(SLogoff { id, graceful: false }));
                }
            }
            ClientToServer::Message(message) => {
                let is_logged_in = player_info.logged_in;
                if is_logged_in {
                    process_message_logged_in(&mut commands, player_info.entity, id, message, &players, &positions);
                } else {
                    process_message_not_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &positions,
                        &speeds,
                        &face_dirs,
                        &velocities,
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
    positions: &Query<&Position>,
    speeds: &Query<&Speed>,
    face_dirs: &Query<&FaceDirection>,
    velocities: &Query<&Velocity>,
    players: &mut ResMut<PlayerMap>,
    grid_config: &Res<GridConfig>,
    items: &Res<ItemMap>,
    ghosts: &Res<crate::resources::GhostMap>,
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
                walls: grid_config.walls.clone(),
                roofs: grid_config.roofs.clone(),
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
            };

            // Construct the initial Update for the new player
            let mut all_players = snapshot_logged_in_players(players, positions, speeds, face_dirs)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial update
            let all_items: Vec<(ItemId, Item)> = items
                .0
                .iter()
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
                .collect();

            // Collect all ghosts for the initial update
            let all_ghosts: Vec<(GhostId, Ghost)> = ghosts
                .0
                .iter()
                .map(|(id, info)| {
                    let pos_component = positions.get(info.entity).expect("Ghost entity missing Position");
                    let vel_component = velocities.get(info.entity).expect("Ghost entity missing Velocity");
                    (
                        *id,
                        Ghost {
                            pos: *pos_component,
                            vel: *vel_component,
                        },
                    )
                })
                .collect();

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
            broadcast_to_logged_in(players, id, ServerMessage::Login(login_msg));
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
    positions: &Query<&Position>,
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
            broadcast_to_logged_in(players, id, ServerMessage::Logoff(SLogoff { id, graceful: true }));
        }
        ClientMessage::Speed(msg) => {
            trace!("{:?} speed: {:?}", id, msg);
            handle_speed(commands, entity, id, msg, players, positions);
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_direction(commands, entity, id, msg, players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot(commands, entity, id, msg, players, positions);
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
        broadcast_to_logged_in(
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

    broadcast_to_logged_in(players, id, ServerMessage::Face(SFace { id, dir: msg.dir }));
}

fn handle_shot(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &PlayerMap,
    positions: &Query<&Position>,
) {
    // Update the shooter's face direction to exact facing direction
    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    // Spawn projectile(s) on server for hit detection
    if let Ok(pos) = positions.get(entity) {
        // Determine number of shots based on multi-shot power-up
        let num_shots = if players
            .0
            .get(&id)
            .map_or(false, |info| info.multi_shot_power_up_timer > 0.0)
        {
            MULTI_SHOT_MULTIPLER
        } else {
            1
        };

        // Spawn projectiles in an arc
        use common::constants::MULTI_SHOT_ANGLE;
        let angle_step = MULTI_SHOT_ANGLE.to_radians();
        let start_offset = -(num_shots - 1) as f32 * angle_step / 2.0;

        for i in 0..num_shots {
            let angle_offset = start_offset + i as f32 * angle_step;
            let shot_dir = msg.face_dir + angle_offset;
            let spawn_pos = Projectile::calculate_spawn_position(Vec3::new(pos.x, pos.y, pos.z), shot_dir);
            let projectile = Projectile::new(shot_dir);

            commands.spawn((
                Position {
                    x: spawn_pos.x,
                    y: spawn_pos.y,
                    z: spawn_pos.z,
                },
                projectile,
                id, // Tag projectile with shooter's ID
            ));
        }
    }

    // Broadcast shot with face direction to all other logged-in players
    broadcast_to_logged_in(
        players,
        id,
        ServerMessage::Shot(SShot {
            id,
            face_dir: msg.face_dir,
        }),
    );
}

// Broadcast authoritative game state in regular time intervals
pub fn broadcast_state_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seq: Local<u32>,
    positions: Query<&Position>,
    speeds: Query<&Speed>,
    face_dirs: Query<&FaceDirection>,
    velocities: Query<&Velocity>,
    players: Res<PlayerMap>,
    items: Res<ItemMap>,
    ghosts: Res<crate::resources::GhostMap>,
) {
    *timer += time.delta_secs();
    if *timer < UPDATE_BROADCAST_INTERVAL {
        return;
    }
    *timer = 0.0;

    // Increment sequence number
    *seq = seq.wrapping_add(1);

    // Collect all logged-in players
    let all_players = snapshot_logged_in_players(&players, &positions, &speeds, &face_dirs);

    // Collect all items
    let all_items: Vec<(ItemId, Item)> = items
        .0
        .iter()
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
        .collect();

    // Collect all ghosts
    let all_ghosts: Vec<(GhostId, Ghost)> = ghosts
        .0
        .iter()
        .map(|(id, info)| {
            let pos_component = positions.get(info.entity).expect("Ghost entity missing Position");
            let vel_component = velocities.get(info.entity).expect("Ghost entity missing Velocity");
            (
                *id,
                Ghost {
                    pos: *pos_component,
                    vel: *vel_component,
                },
            )
        })
        .collect();

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate {
        seq: *seq,
        players: all_players,
        items: all_items,
        ghosts: all_ghosts,
    });
    //trace!("broadcasting update: {:?}", msg);
    for info in players.0.values() {
        if info.logged_in {
            let _ = info.channel.send(ServerToClient::Send(msg.clone()));
        }
    }
}

// Hit detection system - authoritative collision detection
pub fn hit_detection_system(
    mut commands: Commands,
    time: Res<Time>,
    projectile_query: Query<(Entity, &Position, &Projectile, &PlayerId)>,
    player_query: Query<(&Position, &FaceDirection, &PlayerId), Without<Projectile>>,
    grid_config: Res<GridConfig>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, proj_pos, projectile, shooter_id) in projectile_query.iter() {
        let mut hit_something = false;

        // Check wall collisions first
        for wall in &grid_config.walls {
            if common::collision::check_projectile_wall_hit(proj_pos, projectile, delta, wall) {
                // Despawn the projectile when it hits a wall
                commands.entity(proj_entity).despawn();
                hit_something = true;
                break;
            }
        }

        if hit_something {
            continue; // Move to next projectile
        }

        // Check player collisions
        for (player_pos, player_face_dir, target_id) in player_query.iter() {
            // Don't hit yourself
            if shooter_id == target_id {
                continue;
            }

            // Use common hit detection logic
            let result = check_projectile_player_hit(proj_pos, projectile, delta, player_pos, player_face_dir.0);

            if result.hit {
                info!("{:?} hits {:?}", shooter_id, target_id);

                // Update hit counters
                if let Some(shooter_info) = players.0.get_mut(shooter_id) {
                    shooter_info.hits += 1;
                    info!("  {:?} now has {} hits", shooter_id, shooter_info.hits);
                }
                if let Some(target_info) = players.0.get_mut(target_id) {
                    target_info.hits -= 1;
                    info!("  {:?} now has {} hits", target_id, target_info.hits);
                }

                // Broadcast hit message to all clients
                for player_info in players.0.values() {
                    let _ = player_info.channel.send(ServerToClient::Send(ServerMessage::Hit(SHit {
                        id: *target_id,
                        hit_dir_x: result.hit_dir_x,
                        hit_dir_z: result.hit_dir_z,
                    })));
                }

                // Despawn the projectile
                commands.entity(proj_entity).despawn();

                break; // Projectile can only hit one player
            }
        }
    }
}

// ============================================================================
// Movement System (Server with Wall Collision)
// ============================================================================

pub fn server_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut query: Query<(Entity, &mut Position, &Speed, &PlayerId)>,
) {
    let delta = time.delta_secs();

    // Pass 1: Calculate all intended new positions (after wall collision check with sliding)
    let mut intended_positions: Vec<(Entity, Position)> = Vec::new();

    for (entity, pos, speed, player_id) in query.iter() {
        // Convert Speed to Velocity
        let mut velocity = speed.to_velocity();

        // Apply speed power-up multiplier if active
        if let Some(player_info) = players.0.get(player_id) {
            if player_info.speed_power_up_timer > 0.0 {
                velocity.x *= SPEED_POWER_UP_MULTIPLIER;
                velocity.z *= SPEED_POWER_UP_MULTIPLIER;
            }
        }

        let speed = velocity.x.hypot(velocity.z);

        if speed > 0.0 {
            // Calculate new position
            let new_pos = Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z),
            };

            // Check if new position collides with any wall
            let collides_with_wall = grid_config
                .walls
                .iter()
                .any(|wall| check_player_wall_collision(&new_pos, wall));

            // Store intended position (slide along wall if collision, new otherwise)
            if collides_with_wall {
                let slide_pos = common::collision::calculate_wall_slide(
                    &grid_config.walls,
                    pos,
                    &new_pos,
                    velocity.x,
                    velocity.z,
                    delta,
                );
                intended_positions.push((entity, slide_pos));
            } else {
                intended_positions.push((entity, new_pos));
            }
        } else {
            // Not moving, keep current position
            intended_positions.push((entity, *pos));
        }
    }

    // Pass 2: Check player-player collisions and apply positions
    for (entity, intended_pos) in &intended_positions {
        // Check if intended position collides with any other player's intended position
        let collides_with_player = intended_positions.iter().any(|(other_entity, other_intended_pos)| {
            *other_entity != *entity && check_player_player_collision(intended_pos, other_intended_pos)
        });

        if !collides_with_player {
            // No collision, apply intended position
            if let Ok((_, mut pos, _, _)) = query.get_mut(*entity) {
                *pos = *intended_pos;
            }
        }
        // If collision, don't update position (stays at current)
    }
}

// ============================================================================
// Item Systems
// ============================================================================

// System to spawn items at regular intervals
pub fn item_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    positions: Query<&Position>,
) {
    let delta = time.delta_secs();
    spawner.timer += delta;

    if spawner.timer >= ITEM_SPAWN_INTERVAL {
        spawner.timer = 0.0;

        // Get occupied grid cells from existing items
        let occupied_cells: std::collections::HashSet<(i32, i32)> = items
            .0
            .values()
            .filter_map(|info| {
                positions.get(info.entity).ok().map(|pos| {
                    let grid_x = ((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
                    let grid_z = ((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;
                    (grid_x, grid_z)
                })
            })
            .collect();

        // Find an unoccupied grid cell
        let mut rng = rand::rng();
        let max_attempts = 100;

        for _ in 0..max_attempts {
            let grid_x = rng.random_range(0..GRID_COLS);
            let grid_z = rng.random_range(0..GRID_ROWS);

            if !occupied_cells.contains(&(grid_x, grid_z)) {
                // Spawn item in the center of this grid cell
                let world_x = (grid_x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0;
                let world_z = (grid_z as f32 + 0.5) * GRID_SIZE - FIELD_DEPTH / 2.0;

                let item_type = if rng.random_bool(0.5) {
                    ItemType::SpeedPowerUp
                } else {
                    ItemType::MultiShotPowerUp
                };

                let item_id = ItemId(spawner.next_id);
                spawner.next_id += 1;

                let entity = commands
                    .spawn((
                        item_id,
                        Position {
                            x: world_x,
                            y: 0.0,
                            z: world_z,
                        },
                    ))
                    .id();

                items.0.insert(
                    item_id,
                    ItemInfo {
                        entity,
                        item_type,
                        spawn_time: time.elapsed_secs(),
                    },
                );

                break;
            }
        }
    }
}

// System to despawn old items
pub fn item_despawn_system(mut commands: Commands, time: Res<Time>, mut items: ResMut<ItemMap>) {
    let current_time = time.elapsed_secs();

    // Collect items to remove
    let items_to_remove: Vec<ItemId> = items
        .0
        .iter()
        .filter(|(_, info)| current_time - info.spawn_time >= ITEM_LIFETIME)
        .map(|(id, _)| *id)
        .collect();

    // Remove expired items
    for item_id in items_to_remove {
        if let Some(info) = items.0.remove(&item_id) {
            commands.entity(info.entity).despawn();
        }
    }
}

// System to detect player-item collisions and grant items
pub fn item_collection_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    player_positions: Query<&Position, With<PlayerId>>,
    item_positions: Query<&Position, With<ItemId>>,
) {
    // Check each item against each player
    let items_to_collect: Vec<(PlayerId, ItemId, ItemType)> = items
        .0
        .iter()
        .filter_map(|(item_id, item_info)| {
            let item_pos = item_positions.get(item_info.entity).ok()?;

            // Check against all players
            for (player_id, player_info) in &players.0 {
                if let Ok(player_pos) = player_positions.get(player_info.entity) {
                    if check_player_item_collision(player_pos, item_pos, ITEM_COLLECTION_RADIUS) {
                        return Some((*player_id, *item_id, item_info.item_type));
                    }
                }
            }
            None
        })
        .collect();

    // Process collections
    let mut power_up_messages = Vec::new();

    for (player_id, item_id, item_type) in items_to_collect {
        // Remove the item from the map
        if let Some(item_info) = items.0.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }

        // Update player's power-up timer
        if let Some(player_info) = players.0.get_mut(&player_id) {
            match item_type {
                ItemType::SpeedPowerUp => {
                    player_info.speed_power_up_timer = SPEED_POWER_UP_DURATION;
                }
                ItemType::MultiShotPowerUp => {
                    player_info.multi_shot_power_up_timer = MULTI_SHOT_POWER_UP_DURATION;
                }
            }

            power_up_messages.push(SPowerUp {
                id: player_id,
                speed_power_up: player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: player_info.multi_shot_power_up_timer > 0.0,
            });

            debug!("Player {:?} collected {:?}", player_id, item_type);
        }
    }

    // Send power-up updates to all clients
    for msg in power_up_messages {
        broadcast_to_all(&players, ServerMessage::PowerUp(msg));
    }
}

// System to expire player items over time
pub fn item_expiration_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut power_up_messages = Vec::new();

    for (player_id, player_info) in &mut players.0 {
        let old_speed = player_info.speed_power_up_timer > 0.0;
        let old_multi_shot = player_info.multi_shot_power_up_timer > 0.0;

        // Decrease power-up timers
        player_info.speed_power_up_timer = (player_info.speed_power_up_timer - delta).max(0.0);
        player_info.multi_shot_power_up_timer = (player_info.multi_shot_power_up_timer - delta).max(0.0);

        let new_speed = player_info.speed_power_up_timer > 0.0;
        let new_multi_shot = player_info.multi_shot_power_up_timer > 0.0;

        // Track changes to broadcast
        if old_speed != new_speed || old_multi_shot != new_multi_shot {
            power_up_messages.push(SPowerUp {
                id: *player_id,
                speed_power_up: new_speed,
                multi_shot_power_up: new_multi_shot,
            });
        }
    }

    // Send power-up updates to all clients
    for msg in power_up_messages {
        broadcast_to_all(&players, ServerMessage::PowerUp(msg));
    }
}

// ============================================================================
// Ghost Systems
// ============================================================================

// Helper function to get direction from velocity
fn get_direction_from_velocity(vel: &Velocity) -> Option<i32> {
    if vel.x > 0.0 {
        Some(0) // Right (east)
    } else if vel.x < 0.0 {
        Some(2) // Left (west)
    } else if vel.z < 0.0 {
        Some(1) // Up (north)
    } else if vel.z > 0.0 {
        Some(3) // Down (south)
    } else {
        None // Not moving
    }
}

// Helper function to create velocity from direction
fn velocity_from_direction(direction: i32) -> Velocity {
    match direction {
        0 => Velocity {
            x: GHOST_SPEED,
            y: 0.0,
            z: 0.0,
        }, // Right
        1 => Velocity {
            x: 0.0,
            y: 0.0,
            z: -GHOST_SPEED,
        }, // Up
        2 => Velocity {
            x: -GHOST_SPEED,
            y: 0.0,
            z: 0.0,
        }, // Left
        _ => Velocity {
            x: 0.0,
            y: 0.0,
            z: GHOST_SPEED,
        }, // Down
    }
}

// Helper function to check if a direction is blocked by a wall
fn is_direction_blocked(cell: &crate::resources::GridCell, direction: i32) -> bool {
    match direction {
        0 => cell.has_east_wall,  // Right
        1 => cell.has_north_wall, // Up
        2 => cell.has_west_wall,  // Left
        3 => cell.has_south_wall, // Down
        _ => true,
    }
}

// Helper function to get all valid (non-blocked) directions
fn get_valid_directions(cell: &crate::resources::GridCell) -> Vec<i32> {
    let mut valid = Vec::new();
    if !cell.has_east_wall {
        valid.push(0);
    } // Right
    if !cell.has_north_wall {
        valid.push(1);
    } // Up
    if !cell.has_west_wall {
        valid.push(2);
    } // Left
    if !cell.has_south_wall {
        valid.push(3);
    } // Down
    valid
}

// Helper function to filter out backward direction
fn get_forward_directions(valid_directions: &[i32], current_direction: i32) -> Vec<i32> {
    let opposite = match current_direction {
        0 => 2, // Right <-> Left
        1 => 3, // Up <-> Down
        2 => 0, // Left <-> Right
        _ => 1, // Down <-> Up
    };
    valid_directions.iter().copied().filter(|&d| d != opposite).collect()
}

// System to spawn initial ghosts on server startup
pub fn ghost_spawn_system(
    mut commands: Commands,
    mut ghosts: ResMut<crate::resources::GhostMap>,
    grid_config: Res<GridConfig>,
    query: Query<&GhostId>,
) {
    // Only spawn if no ghosts exist yet
    if !query.is_empty() {
        return;
    }

    let mut rng = rand::rng();

    for i in 0..NUM_GHOSTS {
        // Find a random grid cell that doesn't intersect walls
        let mut grid_x;
        let mut grid_z;
        let mut attempts = 0;

        loop {
            grid_x = rng.random_range(0..GRID_COLS);
            grid_z = rng.random_range(0..GRID_ROWS);

            // Calculate center of grid cell
            let world_x = (grid_x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0;
            let world_z = (grid_z as f32 + 0.5) * GRID_SIZE - FIELD_DEPTH / 2.0;
            let pos = Position {
                x: world_x,
                y: 0.0,
                z: world_z,
            };

            // Check if position is valid (not in a wall)
            let mut valid = true;
            for wall in &grid_config.walls {
                if check_ghost_wall_collision(&pos, wall) {
                    valid = false;
                    break;
                }
            }

            if valid || attempts > 100 {
                break;
            }
            attempts += 1;
        }

        // Spawn at grid center
        let pos = Position {
            x: (grid_x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0,
            y: 0.0,
            z: (grid_z as f32 + 0.5) * GRID_SIZE - FIELD_DEPTH / 2.0,
        };

        // Random initial velocity direction (only horizontal or vertical)
        let direction = rng.random_range(0..4); // 0=right, 1=up, 2=left, 3=down
        let vel = match direction {
            0 => Velocity {
                x: GHOST_SPEED,
                y: 0.0,
                z: 0.0,
            }, // Right
            1 => Velocity {
                x: 0.0,
                y: 0.0,
                z: -GHOST_SPEED,
            }, // Up (negative Z)
            2 => Velocity {
                x: -GHOST_SPEED,
                y: 0.0,
                z: 0.0,
            }, // Left
            _ => Velocity {
                x: 0.0,
                y: 0.0,
                z: GHOST_SPEED,
            }, // Down (positive Z)
        };

        let ghost_id = GhostId(i);
        let entity = commands.spawn((ghost_id, pos, vel)).id();

        ghosts.0.insert(ghost_id, crate::resources::GhostInfo { entity });
    }
}

// System to move ghosts with wall avoidance (Pac-Man style)
pub fn ghost_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut ghost_query: Query<(&GhostId, &mut Position, &mut Velocity)>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    for (ghost_id, mut pos, mut vel) in ghost_query.iter_mut() {
        // Calculate which grid cell we're in
        let grid_x = ((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
        let grid_z = ((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;

        // Check if ghost is within grid bounds
        if grid_x < 0 || grid_x >= GRID_COLS || grid_z < 0 || grid_z >= GRID_ROWS {
            error!(
                "{:?} out of bounds at grid ({}, {}), clamping",
                ghost_id, grid_x, grid_z
            );
            // Clamp position to grid bounds
            pos.x = pos.x.clamp(
                -FIELD_WIDTH / 2.0 + GRID_SIZE / 2.0,
                FIELD_WIDTH / 2.0 - GRID_SIZE / 2.0,
            );
            pos.z = pos.z.clamp(
                -FIELD_DEPTH / 2.0 + GRID_SIZE / 2.0,
                FIELD_DEPTH / 2.0 - GRID_SIZE / 2.0,
            );
            // Reverse velocity to bounce back
            vel.x = -vel.x;
            vel.z = -vel.z;
            continue;
        }

        // Calculate grid cell center
        let grid_center_x = (grid_x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0;
        let grid_center_z = (grid_z as f32 + 0.5) * GRID_SIZE - FIELD_DEPTH / 2.0;

        // Check if we're at grid center (within small threshold)
        const CENTER_THRESHOLD: f32 = 0.1;
        let at_center_x = (pos.x - grid_center_x).abs() < CENTER_THRESHOLD;
        let at_center_z = (pos.z - grid_center_z).abs() < CENTER_THRESHOLD;
        let at_intersection = at_center_x && at_center_z;

        if at_intersection {
            let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
            let valid_directions = get_valid_directions(cell);
            let mut direction_changed = false;

            if let Some(current_direction) = get_direction_from_velocity(&vel) {
                if is_direction_blocked(cell, current_direction) {
                    let forward_directions = get_forward_directions(&valid_directions, current_direction);
                    if forward_directions.is_empty() {
                        let new_direction = *valid_directions.first().expect("no valid direction");
                        *vel = velocity_from_direction(new_direction);
                        direction_changed = true;
                    } else {
                        let new_direction = forward_directions[rng.random_range(0..forward_directions.len())];
                        *vel = velocity_from_direction(new_direction);
                        direction_changed = true;
                    }
                }
            }

            if rng.random_bool(GHOST_RANDOM_TURN_PROBABILITY) && !valid_directions.is_empty() {
                let new_direction = valid_directions[rng.random_range(0..valid_directions.len())];
                *vel = velocity_from_direction(new_direction);
                direction_changed = true;
            }

            // Broadcast once after final direction is determined
            if direction_changed {
                broadcast_to_all(
                    &players,
                    ServerMessage::Ghost(SGhost {
                        id: *ghost_id,
                        ghost: Ghost { pos: *pos, vel: *vel },
                    }),
                );
            }
        }

        // Always move based on current velocity
        pos.x += vel.x * delta;
        pos.z += vel.z * delta;

        // Snap to grid line if we're moving along it
        if vel.x.abs() > 0.0 && vel.z.abs() < 0.01 {
            // Moving horizontally - snap Z to grid center
            pos.z = grid_center_z;
        } else if vel.z.abs() > 0.0 && vel.x.abs() < 0.01 {
            // Moving vertically - snap X to grid center
            pos.x = grid_center_x;
        }
    }
}
