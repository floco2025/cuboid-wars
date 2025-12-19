use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::*,
    map::cell_center,
    net::ServerToClient,
    resources::{GhostInfo, GhostMap, GhostMode, PlayerMap},
};
use common::protocol::{GridCell, MapLayout};
use common::{
    collision::{
        ghosts::{
            overlap_ghost_vs_player, slide_ghost_along_obstacles, sweep_ghost_vs_ramp_footprint, sweep_ghost_vs_wall,
        },
        players::sweep_player_vs_wall,
    },
    constants::*,
    markers::{GhostMarker, PlayerMarker},
    protocol::*,
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

fn valid_directions(grid_config: &MapLayout, grid_x: i32, grid_z: i32, cell: GridCell) -> Vec<GridDirection> {
    assert!(
        grid_x >= 0 && grid_x < GRID_COLS && grid_z >= 0 && grid_z < GRID_ROWS,
        "ghost current cell OOB in valid_directions: ({}, {})",
        grid_x,
        grid_z
    );

    // Prefer non-ramp exits; we expect at least one exists for a non-ramp cell
    let open: Vec<_> = GridDirection::ALL
        .iter()
        .copied()
        .filter(|dir| !dir.is_blocked(cell))
        .collect();

    assert!(!open.is_empty(), "no open directions from grid cell");

    let ramp_safe: Vec<_> = open
        .iter()
        .copied()
        .filter(|dir| !direction_leads_to_ramp(grid_config, grid_x, grid_z, *dir))
        .collect();

    assert!(!ramp_safe.is_empty(), "all open directions lead to ramps");

    ramp_safe
}

fn direction_leads_to_ramp(grid_config: &MapLayout, grid_x: i32, grid_z: i32, dir: GridDirection) -> bool {
    assert!(
        grid_x >= 0 && grid_x < GRID_COLS && grid_z >= 0 && grid_z < GRID_ROWS,
        "ghost current cell OOB in direction_leads_to_ramp: ({}, {})",
        grid_x,
        grid_z
    );

    let (next_x, next_z) = match dir {
        GridDirection::East => (grid_x + 1, grid_z),
        GridDirection::North => (grid_x, grid_z - 1),
        GridDirection::West => (grid_x - 1, grid_z),
        GridDirection::South => (grid_x, grid_z + 1),
    };

    if next_x < 0 || next_x >= GRID_COLS || next_z < 0 || next_z >= GRID_ROWS {
        return true; // out-of-bounds neighbor is considered blocked
    }

    grid_config.grid[next_z as usize][next_x as usize].has_ramp
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
    grid_config: Res<MapLayout>,
    query: Query<&GhostId>,
) {
    // Only spawn if no ghosts exist yet
    if !query.is_empty() {
        return;
    }

    let mut rng = rand::rng();

    for i in 0..GHOSTS_NUM {
        // Pick a random grid cell that doesn't have a ramp
        let (grid_x, grid_z) = loop {
            let x = rng.random_range(0..GRID_COLS);
            let z = rng.random_range(0..GRID_ROWS);

            // Check if cell has a ramp
            if !grid_config.grid[z as usize][x as usize].has_ramp {
                break (x, z);
            }
            // If all cells have ramps (unlikely), this would loop forever,
            // but in practice there are many non-ramp cells
        };

        // Spawn at grid center
        let pos = cell_center(grid_x, grid_z);

        // Pick a valid direction based on the cell's walls
        let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
        let valid_directions = valid_directions(&grid_config, grid_x, grid_z, *cell);
        let direction = pick_direction(&mut rng, &valid_directions).expect("no valid direction");
        let vel = direction.to_velocity();

        let ghost_id = GhostId(i);
        let entity = commands.spawn((GhostMarker, ghost_id, pos, vel)).id();

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
    grid_config: Res<MapLayout>,
    players: Res<PlayerMap>,
    mut ghosts: ResMut<GhostMap>,
    mut param_set: ParamSet<(
        Query<(&GhostId, &mut Position, &mut Velocity), With<GhostMarker>>,
        Query<(&PlayerId, &Position, &Speed), With<PlayerMarker>>,
    )>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    // Use all_walls for ghost collision (ghosts never go on roofs)
    let ghost_walls = &grid_config.lower_walls;

    // Collect player positions and speeds (excluding stunned players)
    let player_data: Vec<(PlayerId, Position, Speed)> = param_set
        .p1()
        .iter()
        .filter(|(player_id, _, _)| {
            // Filter out stunned players
            players.0.get(player_id).is_some_and(|info| info.stun_timer <= 0.0)
        })
        .map(|(player_id, position, speed)| (*player_id, *position, *speed))
        .collect();

    // First, collect all ghost data and player data we need
    let mut ghost_updates = Vec::new();
    for (ghost_id, ghost_pos, ghost_vel) in param_set.p0().iter() {
        ghost_updates.push((*ghost_id, *ghost_pos, *ghost_vel));
    }

    // Now process ghost updates
    for (ghost_id, mut ghost_pos, mut ghost_vel) in ghost_updates {
        let Some(ghost_info) = ghosts.0.get_mut(&ghost_id) else {
            continue;
        };

        // Handle mode transitions
        match ghost_info.mode {
            GhostMode::Patrol => {
                // Decrement cooldown timer
                ghost_info.mode_timer -= delta;

                // Always check for visible players
                if let Some(target_player_id) = find_visible_moving_player(&ghost_pos, &player_data, &ghost_walls) {
                    let player_has_ghost_hunt = ALWAYS_GHOST_HUNT
                        || players
                            .0
                            .get(&target_player_id)
                            .is_some_and(|info| info.ghost_hunt_power_up_timer > 0.0);

                    // Enter target mode if: player has ghost hunt (flee) OR cooldown expired (attack)
                    if player_has_ghost_hunt || ghost_info.mode_timer <= 0.0 {
                        ghost_info.mode = GhostMode::Target;
                        ghost_info.mode_timer = GHOST_TARGET_DURATION;
                        ghost_info.follow_target = Some(target_player_id);
                    }
                }
            }
            GhostMode::Target => {
                // Check if we're fleeing from a player with ghost hunt power-up
                let is_fleeing = ghost_info
                    .follow_target
                    .and_then(|target_id| players.0.get(&target_id))
                    .is_some_and(|info| ALWAYS_GHOST_HUNT || info.ghost_hunt_power_up_timer > 0.0);

                // Update target timer: only decrement when not fleeing
                if is_fleeing {
                    // If a ghost was attacking and is now fleeing, the timer has been decremented
                    // previously, so we reset it every time we are fleeing
                    ghost_info.mode_timer = GHOST_TARGET_DURATION;
                } else {
                    ghost_info.mode_timer -= delta;
                }

                if ghost_info.mode_timer <= 0.0 {
                    // Target timer expired, switch to pre-patrol with cooldown
                    ghost_info.mode = GhostMode::PrePatrol;
                    ghost_info.mode_timer = GHOST_COOLDOWN_DURATION;
                    ghost_info.follow_target = None;
                } else {
                    // Check if target player still exists and is not stunned
                    if let Some(target_id) = ghost_info.follow_target {
                        let target_info = players.0.get(&target_id);
                        let target_valid = target_info.is_some_and(|info| info.logged_in && info.stun_timer <= 0.0);
                        let target_on_roof = player_data
                            .iter()
                            .find(|(id, _, _)| *id == target_id)
                            .is_some_and(|(_, pos, _)| pos.y >= ROOF_HEIGHT);

                        if !target_valid || target_on_roof {
                            // Target disconnected, stunned, or on a roof, switch to pre-patrol
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
            GhostMode::Target => {
                if let Some(target_id) = ghost_info.follow_target {
                    follow_movement(
                        &ghost_id,
                        &mut ghost_pos,
                        &mut ghost_vel,
                        target_id,
                        &player_data,
                        &ghost_walls,
                        &grid_config,
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

        // Ignore players that are on or above the roof
        if player_pos.y >= ROOF_HEIGHT {
            continue;
        }

        let distance = (player_pos.x - ghost_pos.x).hypot(player_pos.z - ghost_pos.z);

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
        if sweep_player_vs_wall(from, to, wall) {
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
    grid_config: &MapLayout,
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
        let valid_directions = valid_directions(&grid_config, grid_x, grid_z, *cell);
        let new_direction = pick_direction(rng, &valid_directions).expect("no valid direction");
        *vel = new_direction.to_velocity();
        ghost_info.mode = GhostMode::Patrol;
        ghost_info.mode_timer = GHOST_COOLDOWN_DURATION; // Set cooldown before can detect players again

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
        let distance = dx.hypot(dz);

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
    grid_config: &MapLayout,
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
        let valid_directions = valid_directions(&grid_config, grid_x, grid_z, *cell);
        let mut direction_changed = false;

        if current_direction.is_blocked(*cell)
            || direction_leads_to_ramp(&grid_config, grid_x, grid_z, current_direction)
        {
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
// If the target player has ghost hunt power-up, reverses direction to flee
fn follow_movement(
    ghost_id: &GhostId,
    pos: &mut Position,
    vel: &mut Velocity,
    target_id: PlayerId,
    player_data: &[(PlayerId, Position, Speed)],
    walls: &[Wall],
    grid_config: &MapLayout,
    players: &PlayerMap,
    delta: f32,
) {
    // Find target player position
    let target_pos = player_data
        .iter()
        .find(|(id, pos, _)| *id == target_id && pos.y < ROOF_HEIGHT)
        .map(|(_, pos, _)| pos);

    let Some(target_pos) = target_pos else {
        return;
    };

    // Check if target has ghost hunt power-up active
    let target_has_ghost_hunt = ALWAYS_GHOST_HUNT
        || players
            .0
            .get(&target_id)
            .is_some_and(|info| info.ghost_hunt_power_up_timer > 0.0);

    // Calculate direction to target
    let dx = target_pos.x - pos.x;
    let dz = target_pos.z - pos.z;
    let distance = dx.hypot(dz);

    if distance < 0.01 {
        // Already at target
        vel.x = 0.0;
        vel.z = 0.0;
        return;
    }

    // Normalize direction
    let dir_x = dx / distance;
    let dir_z = dz / distance;

    // If target has ghost hunt power-up, reverse direction (flee instead of follow)
    let (final_dir_x, final_dir_z) = if target_has_ghost_hunt {
        (-dir_x, -dir_z)
    } else {
        (dir_x, dir_z)
    };

    // Apply follow speed
    let desired_vel = Velocity {
        x: final_dir_x * GHOST_FOLLOW_SPEED,
        y: 0.0,
        z: final_dir_z * GHOST_FOLLOW_SPEED,
    };

    // Calculate target position for this frame
    let target_frame_pos = Position {
        x: desired_vel.x.mul_add(delta, pos.x),
        y: 0.0,
        z: desired_vel.z.mul_add(delta, pos.z),
    };

    // Check for wall collisions and apply sliding
    let mut final_pos = target_frame_pos;
    let mut collides = false;

    for wall in walls {
        if sweep_ghost_vs_wall(pos, &final_pos, wall) {
            collides = true;
            break;
        }
    }

    if !collides {
        for ramp in &grid_config.ramps {
            if sweep_ghost_vs_ramp_footprint(pos, &final_pos, ramp) {
                collides = true;
                break;
            }
        }
    }

    if collides {
        final_pos = slide_ghost_along_obstacles(walls, &grid_config.ramps, pos, desired_vel.x, desired_vel.z, delta);
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
    ghost_query: Query<(&GhostId, &Position), With<GhostMarker>>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
) {
    // Collect ghost positions
    let ghost_positions: Vec<(GhostId, Position)> = ghost_query.iter().map(|(id, pos)| (*id, *pos)).collect();

    // Collect player collisions first
    let mut player_hits: Vec<(PlayerId, GhostId)> = Vec::new();

    for (player_id, player_position) in &player_query {
        let Some(player_info) = players.0.get(player_id) else {
            continue;
        };

        // Skip if already stunned
        if player_info.stun_timer > 0.0 {
            continue;
        }

        // Skip if player has hunt power-up
        if ALWAYS_GHOST_HUNT || player_info.ghost_hunt_power_up_timer > 0.0 {
            continue;
        }

        // Check collision with any ghost
        for (ghost_id, ghost_pos) in &ghost_positions {
            let Some(ghost_info) = ghosts.0.get(ghost_id) else {
                continue;
            };

            if ghost_info.mode != GhostMode::Target {
                continue; // Skip stunning if ghost is not targeting
            }

            if ghost_info.follow_target != Some(*player_id) {
                continue; // Ghost is targeting someone else
            }

            if overlap_ghost_vs_player(ghost_pos, player_position) {
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

            let status_msg = SPlayerStatus {
                id: player_id,
                speed_power_up: ALWAYS_SPEED || player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: ALWAYS_MULTI_SHOT || player_info.multi_shot_power_up_timer > 0.0,
                reflect_power_up: ALWAYS_REFLECT || player_info.reflect_power_up_timer > 0.0,
                phasing_power_up: ALWAYS_PHASING || player_info.phasing_power_up_timer > 0.0,
                ghost_hunt_power_up: ALWAYS_GHOST_HUNT || player_info.ghost_hunt_power_up_timer > 0.0,
                stunned: true,
            };

            // Send ghost hit message only to the hit player for sound effect
            let _ = player_info
                .channel
                .send(ServerToClient::Send(ServerMessage::GhostHit(SGhostHit {})));

            broadcast_to_all(&players, ServerMessage::PlayerStatus(status_msg));
        }

        // Put ghost into pre-patrol mode after hitting a player (will return to grid center)
        if let Some(ghost_info) = ghosts.0.get_mut(&ghost_id) {
            ghost_info.mode = GhostMode::PrePatrol;
            ghost_info.mode_timer = 0.0;
        }
    }
}
