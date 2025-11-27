#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use rand::Rng as _;

use crate::{
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, PlayerInfo, PlayerMap, WallConfig},
};
use common::constants::*;
use common::protocol::*;

// ============================================================================
// Helper Functions
// ============================================================================

// Check if a player position intersects with any wall
fn position_intersects_wall(pos: &Position, wall: &Wall) -> bool {
    // Player dimensions with some margin
    let player_half_width = PLAYER_WIDTH / 2.0 + 0.1; // Small margin
    let player_half_depth = PLAYER_DEPTH / 2.0 + 0.1;
    
    match wall.orientation {
        WallOrientation::Horizontal => {
            // Wall extends along X axis at (wall.x, wall.z)
            let wall_half_length = WALL_LENGTH / 2.0;
            let wall_half_thickness = WALL_WIDTH / 2.0;
            
            // Check if player AABB overlaps with wall AABB
            let player_min_x = pos.x - player_half_width;
            let player_max_x = pos.x + player_half_width;
            let player_min_z = pos.z - player_half_depth;
            let player_max_z = pos.z + player_half_depth;
            
            let wall_min_x = wall.x - wall_half_length;
            let wall_max_x = wall.x + wall_half_length;
            let wall_min_z = wall.z - wall_half_thickness;
            let wall_max_z = wall.z + wall_half_thickness;
            
            // AABB overlap test
            player_max_x >= wall_min_x && player_min_x <= wall_max_x &&
            player_max_z >= wall_min_z && player_min_z <= wall_max_z
        },
        WallOrientation::Vertical => {
            // Wall extends along Z axis at (wall.x, wall.z)
            let wall_half_length = WALL_LENGTH / 2.0;
            let wall_half_thickness = WALL_WIDTH / 2.0;
            
            // Check if player AABB overlaps with wall AABB
            let player_min_x = pos.x - player_half_width;
            let player_max_x = pos.x + player_half_width;
            let player_min_z = pos.z - player_half_depth;
            let player_max_z = pos.z + player_half_depth;
            
            let wall_min_x = wall.x - wall_half_thickness;
            let wall_max_x = wall.x + wall_half_thickness;
            let wall_min_z = wall.z - wall_half_length;
            let wall_max_z = wall.z + wall_half_length;
            
            // AABB overlap test
            player_max_x >= wall_min_x && player_min_x <= wall_max_x &&
            player_max_z >= wall_min_z && player_min_z <= wall_max_z
        },
    }
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
        let intersects = wall_config.walls.iter().any(|wall| position_intersects_wall(&pos, wall));
        
        if !intersects {
            return pos;
        }
    }
    
    // Fallback: return center if we couldn't find a valid position
    warn!("Could not find spawn position without wall collision after {} attempts, spawning at center", max_attempts);
    Position { x: 0.0, y: 0.0, z: 0.0 }
}

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
                hits: 0,
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
    wall_config: Res<WallConfig>,
    positions: Query<&Position>,
    movements: Query<&Movement>,
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
                    process_message_logged_in(&mut commands, player_info.entity, id, message, &players, &positions);
                } else {
                    process_message_not_logged_in(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &positions,
                        &movements,
                        &mut players,
                        &wall_config,
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
    movements: &Query<&Movement>,
    players: &mut ResMut<PlayerMap>,
    wall_config: &Res<WallConfig>,
) {
    match msg {
        ClientMessage::Login(_) => {
            debug!("{:?} logged in", id);

            let player_info = players
                .0
                .get_mut(&id)
                .expect("process_message_not_logged_in called for unknown player");

            // Send Init to the connecting player (their ID and walls)
            let init_msg = ServerMessage::Init(SInit { 
                id,
                walls: wall_config.walls.clone(),
            });
            if let Err(e) = player_info.channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let pos = generate_spawn_position(wall_config);

            // Calculate initial facing direction toward center (0, 0)
            // face_dir=0 means facing +Z (sin(0)=0 for X, cos(0)=1 for Z)
            // To face from (pos.x, pos.z) toward (0, 0):
            // direction vector: (-pos.x, -pos.z)
            // face_dir such that: sin(face_dir) * |v| = -pos.x and cos(face_dir) * |v| = -pos.z
            let face_dir = (-pos.x).atan2(-pos.z);

            info!(
                "Player {:?} spawned at ({:.1}, {:.1}), facing {:.2} rad ({:.0}°)",
                id,
                pos.x,
                pos.z,
                face_dir,
                face_dir.to_degrees()
            );

            // Initial movement for the new player
            let mov = Movement {
                vel: Velocity::Idle,
                move_dir: 0.0,
                face_dir,
            };

            // Construct player data
            let player = Player {
                pos,
                mov,
                hits: player_info.hits,
            };

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
                    let mov = movements.get(info.entity).ok()?;
                    Some((
                        *player_id,
                        Player {
                            pos: *pos,
                            mov: *mov,
                            hits: info.hits,
                        },
                    ))
                })
                .collect();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate { players: all_players });
            channel.send(ServerToClient::Send(update_msg)).ok();

            // Now update entity: add Position + Movement
            commands.entity(entity).insert((pos, mov));

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
            let broadcast_msg = ServerMessage::Logoff(SLogoff { id, graceful: true });
            for (other_id, other_info) in players.0.iter() {
                if *other_id != id && other_info.logged_in {
                    let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
                }
            }
        }
        ClientMessage::Movement(msg) => {
            trace!("{:?} movement: {:?}", id, msg);
            handle_movement(commands, entity, id, msg, players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot(commands, entity, id, msg, players, &positions);
        }
        ClientMessage::Echo(msg) => {
            trace!("{:?} echo: {:?}", id, msg);
            if let Some(player_info) = players.0.get(&id) {
                let echo_msg = ServerMessage::Echo(SEcho {
                    timestamp: msg.timestamp,
                });
                let _ = player_info.channel.send(ServerToClient::Send(echo_msg));
            }
        }
    }
}

// ============================================================================
// Movement Handlers
// ============================================================================

fn handle_movement(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CMovement, players: &PlayerMap) {
    // Update the player's movement
    commands.entity(entity).insert(msg.mov);

    // Broadcast movement update to all other logged-in players
    let broadcast_msg = ServerMessage::Movement(SMovement { id, mov: msg.mov });
    for (other_id, other_info) in players.0.iter() {
        if *other_id != id && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
        }
    }
}

fn handle_shot(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &PlayerMap,
    positions: &Query<&Position>,
) {
    // Update the shooter's movement to exact facing direction
    commands.entity(entity).insert(msg.mov);

    // Spawn projectile on server for hit detection
    if let Ok(pos) = positions.get(entity) {
        use common::systems::Projectile;

        let spawn_pos = Projectile::calculate_spawn_position(Vec3::new(pos.x, pos.y, pos.z), msg.mov.face_dir);
        let projectile = Projectile::new(msg.mov.face_dir);

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

    // Broadcast shot with movement to all other logged-in players
    let broadcast_msg = ServerMessage::Shot(SShot { id, mov: msg.mov });
    for (other_id, other_info) in players.0.iter() {
        if *other_id != id && other_info.logged_in {
            let _ = other_info.channel.send(ServerToClient::Send(broadcast_msg.clone()));
        }
    }
}

// Broadcast authoritative game state in regular time intervals
pub fn broadcast_state_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    positions: Query<&Position>,
    movements: Query<&Movement>,
    players: Res<PlayerMap>,
) {
    const BROADCAST_INTERVAL: f32 = 1.0;

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
            let mov = movements.get(info.entity).ok()?;
            Some((
                *player_id,
                Player {
                    pos: *pos,
                    mov: *mov,
                    hits: info.hits,
                },
            ))
        })
        .collect();

    // Broadcast to all logged-in clients
    let msg = ServerMessage::Update(SUpdate { players: all_players });
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
    projectile_query: Query<(Entity, &Position, &common::systems::Projectile, &PlayerId)>,
    player_query: Query<(&Position, &Movement, &PlayerId), Without<common::systems::Projectile>>,
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
        for (player_pos, player_mov, target_id) in player_query.iter() {
            // Don't hit yourself
            if shooter_id == target_id {
                continue;
            }

            // Use common hit detection logic
            let result = common::collision::check_projectile_hit(proj_pos, projectile, delta, player_pos, player_mov);
            
            if result.hit {
                info!("Player {:?} hits Player {:?}", shooter_id, target_id);

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
    mut query: Query<(&mut Position, &Movement)>,
) {
    let delta = time.delta_secs();
    
    for (mut pos, mov) in query.iter_mut() {
        // Calculate movement speed based on velocity
        let speed_m_per_sec = match mov.vel {
            Velocity::Idle => 0.0,
            Velocity::Walk => WALK_SPEED,
            Velocity::Run => RUN_SPEED,
        };

        if speed_m_per_sec > 0.0 {
            // Calculate velocity from direction
            let vel_x = mov.move_dir.sin() * speed_m_per_sec;
            let vel_z = mov.move_dir.cos() * speed_m_per_sec;

            // Calculate new position
            let new_pos = Position {
                x: pos.x + vel_x * delta,
                y: pos.y,
                z: pos.z + vel_z * delta,
            };

            // Check if new position collides with any wall
            let collides_with_wall = wall_config.walls.iter().any(|wall| {
                common::collision::check_player_wall_collision(&new_pos, wall)
            });

            // Only update position if no collision
            if !collides_with_wall {
                *pos = new_pos;
            }
            // If collision detected, player stays at current position (stopped by wall)
        }
    }
}
