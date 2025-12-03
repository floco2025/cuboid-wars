use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::*,
    map::cell_center,
    resources::{GhostInfo, GhostMap, GhostMode, GridCell, GridConfig, PlayerMap},
};
use common::{
    collision::{calculate_ghost_wall_slide, check_ghost_player_overlap, check_ghost_wall_overlap, check_player_wall_sweep},
    constants::*,
    protocol::{Ghost, GhostId, PlayerId, Position, SGhost, ServerMessage, Speed, SpeedLevel, Velocity, Wall},
};

use super::network::broadcast_to_all;

// ============================================================================
// Helper Functions
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum GridDirection {
    East,
    North,
    West,
    South,
}

impl GridDirection {
    const ALL: [Self; 4] = [Self::East, Self::North, Self::West, Self::South];

    fn to_velocity(self) -> Velocity {
        match self {
            Self::East => Velocity {
                x: GHOST_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::North => Velocity {
                x: 0.0,
                y: 0.0,
                z: -GHOST_SPEED,
            },
            Self::West => Velocity {
                x: -GHOST_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::South => Velocity {
                x: 0.0,
                y: 0.0,
                z: GHOST_SPEED,
            },
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::East => Self::West,
            Self::North => Self::South,
            Self::West => Self::East,
            Self::South => Self::North,
        }
    }

    const fn is_blocked(self, cell: GridCell) -> bool {
        match self {
            Self::East => cell.has_east_wall,
            Self::North => cell.has_north_wall,
            Self::West => cell.has_west_wall,
            Self::South => cell.has_south_wall,
        }
    }
}

fn direction_from_velocity(vel: &Velocity) -> Option<GridDirection> {
    if vel.x > 0.0 {
        Some(GridDirection::East)
    } else if vel.x < 0.0 {
        Some(GridDirection::West)
    } else if vel.z < 0.0 {
        Some(GridDirection::North)
    } else if vel.z > 0.0 {
        Some(GridDirection::South)
    } else {
        None
    }
}

fn valid_directions(cell: GridCell) -> Vec<GridDirection> {
    GridDirection::ALL
        .iter()
        .copied()
        .filter(|dir| !dir.is_blocked(cell))
        .collect()
}

fn forward_directions(valid: &[GridDirection], current: GridDirection) -> Vec<GridDirection> {
    valid.iter().copied().filter(|dir| *dir != current.opposite()).collect()
}

fn pick_direction<T: rand::Rng>(rng: &mut T, options: &[GridDirection]) -> Option<GridDirection> {
    if options.is_empty() {
        None
    } else {
        Some(options[rng.random_range(0..options.len())])
    }
}

// ============================================================================
// Ghosts Spawn System
// ============================================================================

// System to spawn initial ghosts on server startup
pub fn ghosts_spawn_system(
    mut commands: Commands,
    mut ghosts: ResMut<GhostMap>,
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
            let pos = cell_center(grid_x, grid_z);

            // Check if position is valid (not in a wall)
            let mut valid = true;
            for wall in &grid_config.walls {
                if check_ghost_wall_overlap(&pos, wall) {
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
        let pos = cell_center(grid_x, grid_z);

        // Random initial velocity direction (only horizontal or vertical)
        let direction = pick_direction(&mut rng, &GridDirection::ALL).unwrap_or(GridDirection::East);
        let vel = direction.to_velocity();

        let ghost_id = GhostId(i);
        let entity = commands.spawn((ghost_id, pos, vel)).id();

        ghosts.0.insert(
            ghost_id,
            GhostInfo {
                entity,
                mode: GhostMode::Patrol,
                mode_timer: 0.0,
                follow_target: None,
                at_intersection: true, // Spawned at grid center
            },
        );
    }
}

// ============================================================================
// Ghosts Movement System
// ============================================================================

pub fn ghosts_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut ghosts: ResMut<GhostMap>,
    mut param_set: ParamSet<(
        Query<(&GhostId, &mut Position, &mut Velocity)>,
        Query<(&PlayerId, &Position, &Speed), With<PlayerId>>,
    )>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    // First, collect all ghost data and player data we need
    let mut ghost_updates = Vec::new();

    // Collect player positions and speeds
    let player_data: Vec<(PlayerId, Position, Speed)> = param_set
        .p1()
        .iter()
        .map(|(id, pos, speed)| (*id, *pos, *speed))
        .collect();

    // Process each ghost
    for (ghost_id, ghost_pos, ghost_vel) in param_set.p0().iter() {
        ghost_updates.push((*ghost_id, *ghost_pos, *ghost_vel));
    }

    // Now process ghost updates
    for (ghost_id, mut ghost_pos, mut ghost_vel) in ghost_updates {
        let Some(ghost_info) = ghosts.0.get_mut(&ghost_id) else {
            continue;
        };

        // Update mode timer
        ghost_info.mode_timer -= delta;

        // Handle mode transitions
        match ghost_info.mode {
            GhostMode::Patrol => {
                // Only check for visible players if cooldown timer has expired
                if ghost_info.mode_timer <= 0.0 {
                    // Check if we can see any moving players
                    if let Some(target_player_id) = find_visible_moving_player(&ghost_pos, &player_data, &grid_config.walls)
                    {
                        // Switch to follow mode
                        ghost_info.mode = GhostMode::Follow;
                        ghost_info.mode_timer = GHOST_FOLLOW_DURATION;
                        ghost_info.follow_target = Some(target_player_id);
                    }
                }
            }
            GhostMode::Follow => {
                if ghost_info.mode_timer <= 0.0 {
                    // Switch to pre-patrol to navigate to grid
                    ghost_info.mode = GhostMode::PrePatrol;
                    ghost_info.mode_timer = GHOST_COOLDOWN_DURATION;
                    ghost_info.follow_target = None;
                } else {
                    // Check if target player still exists
                    if let Some(target_id) = ghost_info.follow_target {
                        let target_exists = players.0.get(&target_id).is_some_and(|info| info.logged_in);
                        if !target_exists {
                            // Target disconnected, switch to pre-patrol
                            ghost_info.mode = GhostMode::PrePatrol;
                            ghost_info.mode_timer = GHOST_COOLDOWN_DURATION;
                            ghost_info.follow_target = None;
                        }
                    }
                }
            }
            GhostMode::PrePatrol => {
                // PrePatrol doesn't have a timer - it transitions when reaching grid center
                // The transition is handled in pre_patrol_movement
            }
        }

        // Execute movement based on current mode
        match ghost_info.mode {
            GhostMode::PrePatrol => {
                // PrePatrol needs mutable ghost_info to transition state
                pre_patrol_movement(
                    &ghost_id,
                    &mut ghost_pos,
                    &mut ghost_vel,
                    ghost_info,
                    &grid_config,
                    &players,
                    delta,
                    &mut rng,
                );
            }
            GhostMode::Patrol => {
                patrol_movement(
                    &ghost_id,
                    &mut ghost_pos,
                    &mut ghost_vel,
                    ghost_info,
                    &grid_config,
                    &players,
                    delta,
                    &mut rng,
                );
            }
            GhostMode::Follow => {
                if let Some(target_id) = ghost_info.follow_target {
                    follow_movement(
                        &ghost_id,
                        &mut ghost_pos,
                        &mut ghost_vel,
                        target_id,
                        &player_data,
                        &grid_config.walls,
                        &players,
                        delta,
                    );
                }
            }
        }

        // Write back the updated position and velocity
        if let Ok((_, mut pos, mut vel)) = param_set.p0().get_mut(ghost_info.entity) {
            *pos = ghost_pos;
            *vel = ghost_vel;
        }
    }
}

// ============================================================================
// AI Helper Functions
// ============================================================================

// Find the first moving player visible from ghost's position using line-of-sight check
fn find_visible_moving_player(
    ghost_pos: &Position,
    player_data: &[(PlayerId, Position, Speed)],
    walls: &[Wall],
) -> Option<PlayerId> {
    for (player_id, player_pos, player_speed) in player_data {
        // Ignore players that are not moving (Idle speed)
        if player_speed.speed_level == SpeedLevel::Idle {
            continue;
        }

        let distance = ((player_pos.x - ghost_pos.x).powi(2) + (player_pos.z - ghost_pos.z).powi(2)).sqrt();

        if distance > GHOST_VISION_RANGE {
            continue;
        }

        // Check line of sight - use player sweep to check if path is clear
        if has_line_of_sight(ghost_pos, player_pos, walls) {
            return Some(*player_id);
        }
    }
    None
}

// Check if there's a clear line of sight between two positions
fn has_line_of_sight(from: &Position, to: &Position, walls: &[Wall]) -> bool {
    // Use swept collision check to see if any wall blocks the path
    for wall in walls {
        if check_player_wall_sweep(from, to, wall) {
            return false;
        }
    }
    true
}

const GHOST_CENTER_THRESHOLD: f32 = 0.2;

// Pre-patrol mode movement - navigates to grid center before entering patrol
fn pre_patrol_movement(
    ghost_id: &GhostId,
    pos: &mut Position,
    vel: &mut Velocity,
    ghost_info: &mut GhostInfo,
    grid_config: &GridConfig,
    players: &PlayerMap,
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < GHOST_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < GHOST_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    if at_intersection {
        // We've reached the grid center - pick a valid direction and transition to patrol
        let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
        let valid_directions = valid_directions(*cell);
        let new_direction = pick_direction(rng, &valid_directions).expect("no valid direction");
        *vel = new_direction.to_velocity();
        ghost_info.mode = GhostMode::Patrol;

        broadcast_to_all(
            players,
            ServerMessage::Ghost(SGhost {
                id: *ghost_id,
                ghost: Ghost { pos: *pos, vel: *vel },
            }),
        );
    } else {
        // Not at center yet - move directly toward it
        let dx = center.x - pos.x;
        let dz = center.z - pos.z;
        let distance = (dx * dx + dz * dz).sqrt();

        // Normalize and apply ghost speed
        let dir_x = dx / distance;
        let dir_z = dz / distance;
        let new_vel = Velocity {
            x: dir_x * GHOST_SPEED,
            y: 0.0,
            z: dir_z * GHOST_SPEED,
        };

        // Only broadcast if velocity changed
        let vel_changed = (new_vel.x - vel.x).abs() > 0.1 || (new_vel.z - vel.z).abs() > 0.1;

        *vel = new_vel;

        if vel_changed {
            broadcast_to_all(
                players,
                ServerMessage::Ghost(SGhost {
                    id: *ghost_id,
                    ghost: Ghost { pos: *pos, vel: *vel },
                }),
            );
        }

        pos.x += vel.x * delta;
        pos.z += vel.z * delta;
    }
}

// Patrol mode movement - follows grid lines
fn patrol_movement(
    ghost_id: &GhostId,
    pos: &mut Position,
    vel: &mut Velocity,
    ghost_info: &mut GhostInfo,
    grid_config: &GridConfig,
    players: &PlayerMap,
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < GHOST_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < GHOST_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    let just_arrived = at_intersection && !ghost_info.at_intersection;
    ghost_info.at_intersection = at_intersection;

    let mut current_direction = direction_from_velocity(vel).expect("no current direction");

    if just_arrived {
        let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
        let valid_directions = valid_directions(*cell);
        let mut direction_changed = false;

        if current_direction.is_blocked(*cell) {
            let forward_options = forward_directions(&valid_directions, current_direction);
            if forward_options.is_empty() {
                let new_direction = valid_directions.first().copied().expect("no valid direction");
                *vel = new_direction.to_velocity();
                direction_changed = true;
            } else if let Some(new_direction) = pick_direction(rng, &forward_options) {
                *vel = new_direction.to_velocity();
                direction_changed = true;
            }
        }

        if rng.random_bool(GHOST_RANDOM_TURN_PROBABILITY)
            && !valid_directions.is_empty()
            && let Some(new_direction) = pick_direction(rng, &valid_directions)
        {
            *vel = new_direction.to_velocity();
            direction_changed = true;
        }

        if direction_changed {
            broadcast_to_all(
                players,
                ServerMessage::Ghost(SGhost {
                    id: *ghost_id,
                    ghost: Ghost { pos: *pos, vel: *vel },
                }),
            );

            current_direction = direction_from_velocity(vel).expect("no current direction");
        }
    }

    pos.x += vel.x * delta;
    pos.z += vel.z * delta;

    // Incrementally adjust position toward grid line based on current direction
    match current_direction {
        GridDirection::East | GridDirection::West => {
            let diff = center.z - pos.z;
            pos.z += diff.signum() * (diff.abs().min(GHOST_SPEED * delta * 0.5));
        }
        GridDirection::North | GridDirection::South => {
            let diff = center.x - pos.x;
            pos.x += diff.signum() * (diff.abs().min(GHOST_SPEED * delta * 0.5));
        }
    }
}

// Follow mode movement - moves toward target player with wall sliding
fn follow_movement(
    ghost_id: &GhostId,
    pos: &mut Position,
    vel: &mut Velocity,
    target_id: PlayerId,
    player_data: &[(PlayerId, Position, Speed)],
    walls: &[Wall],
    players: &PlayerMap,
    delta: f32,
) {
    // Find target player position
    let target_pos = player_data
        .iter()
        .find(|(id, _, _)| *id == target_id)
        .map(|(_, pos, _)| pos);

    let Some(target_pos) = target_pos else {
        return;
    };

    // Calculate direction to target
    let dx = target_pos.x - pos.x;
    let dz = target_pos.z - pos.z;
    let distance = (dx * dx + dz * dz).sqrt();

    if distance < 0.01 {
        // Already at target
        vel.x = 0.0;
        vel.z = 0.0;
        return;
    }

    // Normalize direction and apply follow speed
    let dir_x = dx / distance;
    let dir_z = dz / distance;
    let desired_vel = Velocity {
        x: dir_x * GHOST_FOLLOW_SPEED,
        y: 0.0,
        z: dir_z * GHOST_FOLLOW_SPEED,
    };

    // Calculate target position for this frame
    let target_frame_pos = Position {
        x: pos.x + desired_vel.x * delta,
        y: 0.0,
        z: pos.z + desired_vel.z * delta,
    };

    // Check for wall collisions and apply sliding
    let mut final_pos = target_frame_pos;
    let mut collides = false;

    for wall in walls {
        if check_ghost_wall_overlap(&final_pos, wall) {
            collides = true;
            break;
        }
    }

    if collides {
        final_pos = calculate_ghost_wall_slide(walls, pos, desired_vel.x, desired_vel.z, delta);
    }

    let actual_dx = final_pos.x - pos.x;
    let actual_dz = final_pos.z - pos.z;
    let new_vel = Velocity {
        x: actual_dx / delta,
        y: 0.0,
        z: actual_dz / delta,
    };

    // Only broadcast if velocity changed significantly
    let vel_changed = (new_vel.x - vel.x).abs() > 0.1 || (new_vel.z - vel.z).abs() > 0.1;

    *vel = new_vel;
    *pos = final_pos;

    if vel_changed {
        broadcast_to_all(
            players,
            ServerMessage::Ghost(SGhost {
                id: *ghost_id,
                ghost: Ghost { pos: *pos, vel: *vel },
            }),
        );
    }
}

// ============================================================================
// Ghost-Player Collision System
// ============================================================================

// Check for ghost-player collisions and apply stun
pub fn ghost_player_collision_system(
    mut ghosts: ResMut<GhostMap>,
    mut players: ResMut<PlayerMap>,
    ghost_query: Query<(&GhostId, &Position)>,
    player_query: Query<(&PlayerId, &Position)>,
) {
    use crate::constants::{GHOST_HIT_PENALTY, GHOST_STUN_DURATION};
    use common::protocol::SPlayerStatus;

    // Collect ghost positions
    let ghost_positions: Vec<(GhostId, Position)> = ghost_query.iter().map(|(id, pos)| (*id, *pos)).collect();

    // Collect player collisions first
    let mut player_hits: Vec<(PlayerId, GhostId)> = Vec::new();

    for (player_id, player_pos) in &player_query {
        let Some(player_info) = players.0.get(player_id) else {
            continue;
        };

        // Skip if already stunned
        if player_info.stun_timer > 0.0 {
            continue;
        }

        // Check collision with any ghost
        for (ghost_id, ghost_pos) in &ghost_positions {
            if check_ghost_player_overlap(ghost_pos, player_pos) {
                player_hits.push((*player_id, *ghost_id));
                break; // Only one hit per frame
            }
        }
    }

    // Apply stun and broadcast
    for (player_id, ghost_id) in player_hits {
        if let Some(player_info) = players.0.get_mut(&player_id) {
            player_info.stun_timer = GHOST_STUN_DURATION;
            player_info.hits -= GHOST_HIT_PENALTY;

            debug!("{:?} was hit by {:?}, stunned for {}s, lost {} points",
                player_id, ghost_id, GHOST_STUN_DURATION, GHOST_HIT_PENALTY);

            let status_msg = SPlayerStatus {
                id: player_id,
                speed_power_up: player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: player_info.multi_shot_power_up_timer > 0.0,
                reflect_power_up: player_info.reflect_power_up_timer > 0.0,
                stunned: true,
            };

            broadcast_to_all(&players, ServerMessage::PlayerStatus(status_msg));
        }

        // Put ghost into pre-patrol mode after hitting a player (will return to grid center)
        if let Some(ghost_info) = ghosts.0.get_mut(&ghost_id) {
            ghost_info.mode = GhostMode::PrePatrol;
            ghost_info.mode_timer = 0.0;
            debug!("{:?} entering PrePatrol after stunning {:?}", ghost_id, player_id);
        }
    }
}
