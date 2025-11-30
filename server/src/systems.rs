use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::{ITEM_LIFETIME, ITEM_SPAWN_INTERVAL},
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, ItemInfo, ItemMap, ItemSpawner, PlayerInfo, PlayerMap, WallConfig},
};
use common::collision::{check_player_player_collision, check_player_wall_collision, check_projectile_player_hit};
use common::constants::*;
use common::protocol::*;
use common::systems::Projectile;

// ============================================================================
// Components
// ============================================================================

// ============================================================================
// Helper Functions
// ============================================================================

fn position_intersects_wall(pos: &Position, wall: &Wall) -> bool {
    const MARGIN: f32 = 0.1;
    let player_half_x = PLAYER_WIDTH / 2.0 + MARGIN;
    let player_half_z = PLAYER_DEPTH / 2.0 + MARGIN;

    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    let player_min_x = pos.x - player_half_x;
    let player_max_x = pos.x + player_half_x;
    let player_min_z = pos.z - player_half_z;
    let player_max_z = pos.z + player_half_z;

    let wall_min_x = wall.x - wall_half_x;
    let wall_max_x = wall.x + wall_half_x;
    let wall_min_z = wall.z - wall_half_z;
    let wall_max_z = wall.z + wall_half_z;

    ranges_intersect(player_min_x, player_max_x, wall_min_x, wall_max_x)
        && ranges_intersect(player_min_z, player_max_z, wall_min_z, wall_max_z)
}

fn ranges_intersect(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> bool {
    a_max >= b_min && a_min <= b_max
}

// Generate a random spawn position that doesn't intersect with any walls
fn generate_spawn_position(wall_config: &WallConfig) -> Position {
    let mut rng = rand::rng();
    let max_attempts = 100;

    for _ in 0..max_attempts {
        let pos = Position {
            x: rng.random_range(-SPAWN_RANGE_X..=SPAWN_RANGE_X),
            y: 0.0,
            z: rng.random_range(-SPAWN_RANGE_Z..=SPAWN_RANGE_Z),
        };

        // Check if position intersects with any wall
        let intersects = wall_config
            .walls
            .iter()
            .any(|wall| position_intersects_wall(&pos, wall));

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
                    items: info.items.clone(),
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
                items: Vec::new(),
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
    wall_config: Res<WallConfig>,
    positions: Query<&Position>,
    speeds: Query<&Speed>,
    face_dirs: Query<&FaceDirection>,
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
                        &mut players,
                        &wall_config,
                        &items,
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
    players: &mut ResMut<PlayerMap>,
    wall_config: &Res<WallConfig>,
    items: &Res<ItemMap>,
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

            // Send Init to the connecting player (their ID and walls)
            let init_msg = ServerMessage::Init(SInit {
                id,
                walls: wall_config.walls.clone(),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let pos = generate_spawn_position(wall_config);

            // Calculate initial facing direction toward center
            let face_dir = (-pos.x).atan2(-pos.z);

            info!(
                "Player {:?} ({name}) spawned at ({:.1}, {:.1}), facing {:.2} rad ({:.0}°)",
                id,
                pos.x,
                pos.z,
                face_dir,
                face_dir.to_degrees()
            );

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
                items: Vec::new(),
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

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate {
                players: all_players,
                items: all_items,
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
            handle_speed(commands, entity, id, msg, players);
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

fn handle_speed(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CSpeed, players: &PlayerMap) {
    // Update the player's speed
    commands.entity(entity).insert(msg.speed);

    // Broadcast speed update to all other logged-in players
    broadcast_to_logged_in(players, id, ServerMessage::Speed(SSpeed { id, speed: msg.speed }));
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

    // Spawn projectile on server for hit detection
    if let Ok(pos) = positions.get(entity) {
        let spawn_pos = Projectile::calculate_spawn_position(Vec3::new(pos.x, pos.y, pos.z), msg.face_dir);
        let projectile = Projectile::new(msg.face_dir);

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
    positions: Query<&Position>,
    speeds: Query<&Speed>,
    face_dirs: Query<&FaceDirection>,
    players: Res<PlayerMap>,
    items: Res<ItemMap>,
) {
    *timer += time.delta_secs();
    if *timer < UPDATE_BROADCAST_INTERVAL {
        return;
    }
    *timer = 0.0;

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

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate {
        players: all_players,
        items: all_items,
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
    wall_config: Res<WallConfig>,
    mut players: ResMut<PlayerMap>,
) {
    let delta = time.delta_secs();

    for (proj_entity, proj_pos, projectile, shooter_id) in projectile_query.iter() {
        let mut hit_something = false;

        // Check wall collisions first
        for wall in &wall_config.walls {
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
    wall_config: Res<WallConfig>,
    mut query: Query<(Entity, &mut Position, &Speed)>,
) {
    let delta = time.delta_secs();

    // Pass 1: Calculate all intended new positions (after wall collision check with sliding)
    let mut intended_positions: Vec<(Entity, Position)> = Vec::new();

    for (entity, pos, speed) in query.iter() {
        // Convert Speed to Velocity
        let velocity = speed.to_velocity();
        let speed = velocity.x.hypot(velocity.z);

        if speed > 0.0 {
            // Calculate new position
            let new_pos = Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z),
            };

            // Check if new position collides with any wall
            let collides_with_wall = wall_config
                .walls
                .iter()
                .any(|wall| check_player_wall_collision(&new_pos, wall));

            // Store intended position (slide along wall if collision, new otherwise)
            if collides_with_wall {
                let slide_pos = common::collision::calculate_wall_slide(
                    &wall_config.walls,
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
            if let Ok((_, mut pos, _)) = query.get_mut(*entity) {
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
                    ItemType::Speed
                } else {
                    ItemType::MultiShot
                };

                let item_id = ItemId(spawner.next_id);
                spawner.next_id += 1;

                let entity = commands.spawn((
                    item_id,
                    Position {
                        x: world_x,
                        y: 0.0,
                        z: world_z,
                    },
                )).id();

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
pub fn item_despawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut items: ResMut<ItemMap>,
) {
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
