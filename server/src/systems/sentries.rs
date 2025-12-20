use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::*,
    map::cell_center,
    net::ServerToClient,
    resources::{SentryInfo, SentryMap, SentryMode, SentrySpawnConfig, GridCell, GridConfig, PlayerMap},
};
use common::{
    collision::{
        sentries::{
            overlap_sentry_vs_player, slide_sentry_along_obstacles, sweep_sentry_vs_ramp_footprint, sweep_sentry_vs_wall,
        },
        players::sweep_player_vs_wall,
    },
    constants::*,
    markers::{SentryMarker, PlayerMarker},
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
                x: SENTRY_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::North => Velocity {
                x: 0.0,
                y: 0.0,
                z: -SENTRY_SPEED,
            },
            Self::West => Velocity {
                x: -SENTRY_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::South => Velocity {
                x: 0.0,
                y: 0.0,
                z: SENTRY_SPEED,
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

fn valid_directions(grid_config: &GridConfig, grid_x: i32, grid_z: i32, cell: GridCell) -> Vec<GridDirection> {
    assert!(
        (0..GRID_COLS).contains(&grid_x) && (0..GRID_ROWS).contains(&grid_z),
        "sentry current cell OOB in valid_directions: ({grid_x}, {grid_z})"
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

fn direction_leads_to_ramp(grid_config: &GridConfig, grid_x: i32, grid_z: i32, dir: GridDirection) -> bool {
    assert!(
        (0..GRID_COLS).contains(&grid_x) && (0..GRID_ROWS).contains(&grid_z),
        "sentry current cell OOB in direction_leads_to_ramp: ({grid_x}, {grid_z})"
    );

    let (next_x, next_z) = match dir {
        GridDirection::East => (grid_x + 1, grid_z),
        GridDirection::North => (grid_x, grid_z - 1),
        GridDirection::West => (grid_x - 1, grid_z),
        GridDirection::South => (grid_x, grid_z + 1),
    };

    if !(0..GRID_COLS).contains(&next_x) || !(0..GRID_ROWS).contains(&next_z) {
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
// Sentries Spawn System
// ============================================================================

// System to spawn initial sentries on server startup
pub fn sentries_spawn_system(
    mut commands: Commands,
    mut sentries: ResMut<SentryMap>,
    grid_config: Res<GridConfig>,
    spawn_config: Res<SentrySpawnConfig>,
    query: Query<&SentryId>,
) {
    // Only spawn if no sentries exist yet
    if !query.is_empty() {
        return;
    }

    let mut rng = rand::rng();

    for i in 0..spawn_config.num_sentries {
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

        let sentry_id = SentryId(i);
        let entity = commands.spawn((SentryMarker, sentry_id, pos, vel)).id();

        sentries.0.insert(
            sentry_id,
            SentryInfo {
                entity,
                mode: SentryMode::Patrol,
                mode_timer: 0.0,
                follow_target: None,
                at_intersection: true, // Spawned at grid center
            },
        );
    }
}

// ============================================================================
// Sentries Movement System
// ============================================================================

pub fn sentries_movement_system(
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut sentries: ResMut<SentryMap>,
    mut param_set: ParamSet<(
        Query<(&SentryId, &mut Position, &mut Velocity), With<SentryMarker>>,
        Query<(&PlayerId, &Position, &Speed), With<PlayerMarker>>,
    )>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    // Use all_walls for sentry collision (sentries never go on roofs)
    let sentry_walls = &map_layout.lower_walls;

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

    // First, collect all sentry data and player data we need
    let mut sentry_updates = Vec::new();
    for (sentry_id, sentry_pos, sentry_vel) in param_set.p0().iter() {
        sentry_updates.push((*sentry_id, *sentry_pos, *sentry_vel));
    }

    // Now process sentry updates
    for (sentry_id, mut sentry_pos, mut sentry_vel) in sentry_updates {
        let Some(sentry_info) = sentries.0.get_mut(&sentry_id) else {
            continue;
        };

        // Handle mode transitions
        match sentry_info.mode {
            SentryMode::Patrol => {
                // Decrement cooldown timer
                sentry_info.mode_timer -= delta;

                // Always check for visible players
                if let Some(target_player_id) = find_visible_moving_player(&sentry_pos, &player_data, sentry_walls) {
                    let player_has_sentry_hunt = ALWAYS_SENTRY_HUNT
                        || players
                            .0
                            .get(&target_player_id)
                            .is_some_and(|info| info.sentry_hunt_power_up_timer > 0.0);

                    // Enter target mode if: player has sentry hunt (flee) OR cooldown expired (attack)
                    if player_has_sentry_hunt || sentry_info.mode_timer <= 0.0 {
                        sentry_info.mode = SentryMode::Target;
                        sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                        sentry_info.follow_target = Some(target_player_id);
                    }
                }
            }
            SentryMode::Target => {
                // Check if we're fleeing from a player with sentry hunt power-up
                let is_fleeing = sentry_info
                    .follow_target
                    .and_then(|target_id| players.0.get(&target_id))
                    .is_some_and(|info| ALWAYS_SENTRY_HUNT || info.sentry_hunt_power_up_timer > 0.0);

                // Update target timer: only decrement when not fleeing
                if is_fleeing {
                    // If a sentry was attacking and is now fleeing, the timer has been decremented
                    // previously, so we reset it every time we are fleeing
                    sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                } else {
                    sentry_info.mode_timer -= delta;
                }

                if sentry_info.mode_timer <= 0.0 {
                    // Target timer expired, switch to pre-patrol with cooldown
                    sentry_info.mode = SentryMode::PrePatrol;
                    sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION;
                    sentry_info.follow_target = None;
                } else {
                    // Check if target player still exists and is not stunned
                    if let Some(target_id) = sentry_info.follow_target {
                        let target_info = players.0.get(&target_id);
                        let target_valid = target_info.is_some_and(|info| info.logged_in && info.stun_timer <= 0.0);
                        let target_on_roof = player_data
                            .iter()
                            .find(|(id, _, _)| *id == target_id)
                            .is_some_and(|(_, pos, _)| pos.y >= ROOF_HEIGHT);

                        if !target_valid || target_on_roof {
                            // Target disconnected, stunned, or on a roof, switch to pre-patrol
                            sentry_info.mode = SentryMode::PrePatrol;
                            sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION;
                            sentry_info.follow_target = None;
                        }
                    }
                }
            }
            SentryMode::PrePatrol => {
                // PrePatrol doesn't have a timer - it transitions when reaching grid center
                // The transition is handled in pre_patrol_movement
            }
        }

        // Execute movement based on current mode
        match sentry_info.mode {
            SentryMode::PrePatrol => {
                pre_patrol_movement(
                    &sentry_id,
                    &mut sentry_pos,
                    &mut sentry_vel,
                    sentry_info,
                    &grid_config,
                    &players,
                    delta,
                    &mut rng,
                );
            }
            SentryMode::Patrol => {
                patrol_movement(
                    &sentry_id,
                    &mut sentry_pos,
                    &mut sentry_vel,
                    sentry_info,
                    &grid_config,
                    &players,
                    delta,
                    &mut rng,
                );
            }
            SentryMode::Target => {
                if let Some(target_id) = sentry_info.follow_target {
                    follow_movement(
                        &sentry_id,
                        &mut sentry_pos,
                        &mut sentry_vel,
                        target_id,
                        &player_data,
                        sentry_walls,
                        &map_layout.ramps,
                        &players,
                        delta,
                    );
                }
            }
        }

        // Write back the updated position and velocity
        if let Ok((_, mut pos, mut vel)) = param_set.p0().get_mut(sentry_info.entity) {
            *pos = sentry_pos;
            *vel = sentry_vel;
        }
    }
}

// ============================================================================
// AI Helper Functions
// ============================================================================

// Find the first moving player visible from sentry's position using line-of-sight check
fn find_visible_moving_player(
    sentry_pos: &Position,
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

        let distance = (player_pos.x - sentry_pos.x).hypot(player_pos.z - sentry_pos.z);

        if distance > SENTRY_VISION_RANGE {
            continue;
        }

        // Check line of sight - use player sweep to check if path is clear
        if has_line_of_sight(sentry_pos, player_pos, walls) {
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

const SENTRY_CENTER_THRESHOLD: f32 = 0.2;

// Pre-patrol mode movement - navigates to grid center before entering patrol
fn pre_patrol_movement(
    sentry_id: &SentryId,
    pos: &mut Position,
    vel: &mut Velocity,
    sentry_info: &mut SentryInfo,
    grid_config: &GridConfig,
    players: &PlayerMap,
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    if at_intersection {
        // We've reached the grid center - pick a valid direction and transition to patrol
        let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
        let valid_directions = valid_directions(grid_config, grid_x, grid_z, *cell);
        let new_direction = pick_direction(rng, &valid_directions).expect("no valid direction");
        *vel = new_direction.to_velocity();
        sentry_info.mode = SentryMode::Patrol;
        sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION; // Set cooldown before can detect players again

        broadcast_to_all(
            players,
            ServerMessage::Sentry(SSentry {
                id: *sentry_id,
                sentry: Sentry { pos: *pos, vel: *vel },
            }),
        );
    } else {
        // Not at center yet - move directly toward it
        let dx = center.x - pos.x;
        let dz = center.z - pos.z;
        let distance = dx.hypot(dz);

        // Normalize and apply sentry speed
        let dir_x = dx / distance;
        let dir_z = dz / distance;
        let new_vel = Velocity {
            x: dir_x * SENTRY_SPEED,
            y: 0.0,
            z: dir_z * SENTRY_SPEED,
        };

        // Only broadcast if velocity changed
        let vel_changed = (new_vel.x - vel.x).abs() > 0.1 || (new_vel.z - vel.z).abs() > 0.1;

        *vel = new_vel;

        if vel_changed {
            broadcast_to_all(
                players,
                ServerMessage::Sentry(SSentry {
                    id: *sentry_id,
                    sentry: Sentry { pos: *pos, vel: *vel },
                }),
            );
        }

        pos.x += vel.x * delta;
        pos.z += vel.z * delta;
    }
}

// Patrol mode movement - follows grid lines
fn patrol_movement(
    sentry_id: &SentryId,
    pos: &mut Position,
    vel: &mut Velocity,
    sentry_info: &mut SentryInfo,
    grid_config: &GridConfig,
    players: &PlayerMap,
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    let just_arrived = at_intersection && !sentry_info.at_intersection;
    sentry_info.at_intersection = at_intersection;

    let mut current_direction = direction_from_velocity(vel).expect("no current direction");

    if just_arrived {
        let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
        let valid_directions = valid_directions(grid_config, grid_x, grid_z, *cell);
        let mut direction_changed = false;

        if current_direction.is_blocked(*cell)
            || direction_leads_to_ramp(grid_config, grid_x, grid_z, current_direction)
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

        if rng.random_bool(SENTRY_RANDOM_TURN_PROBABILITY)
            && !valid_directions.is_empty()
            && let Some(new_direction) = pick_direction(rng, &valid_directions)
        {
            *vel = new_direction.to_velocity();
            direction_changed = true;
        }

        if direction_changed {
            broadcast_to_all(
                players,
                ServerMessage::Sentry(SSentry {
                    id: *sentry_id,
                    sentry: Sentry { pos: *pos, vel: *vel },
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
            pos.z += diff.signum() * (diff.abs().min(SENTRY_SPEED * delta * 0.5));
        }
        GridDirection::North | GridDirection::South => {
            let diff = center.x - pos.x;
            pos.x += diff.signum() * (diff.abs().min(SENTRY_SPEED * delta * 0.5));
        }
    }
}

// Follow mode movement - moves toward target player with wall sliding
// If the target player has sentry hunt power-up, reverses direction to flee
fn follow_movement(
    sentry_id: &SentryId,
    pos: &mut Position,
    vel: &mut Velocity,
    target_id: PlayerId,
    player_data: &[(PlayerId, Position, Speed)],
    walls: &[Wall],
    ramps: &[Ramp],
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

    // Check if target has sentry hunt power-up active
    let target_has_sentry_hunt = ALWAYS_SENTRY_HUNT
        || players
            .0
            .get(&target_id)
            .is_some_and(|info| info.sentry_hunt_power_up_timer > 0.0);

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

    // If target has sentry hunt power-up, reverse direction (flee instead of follow)
    let (final_dir_x, final_dir_z) = if target_has_sentry_hunt {
        (-dir_x, -dir_z)
    } else {
        (dir_x, dir_z)
    };

    // Apply follow speed
    let desired_vel = Velocity {
        x: final_dir_x * SENTRY_FOLLOW_SPEED,
        y: 0.0,
        z: final_dir_z * SENTRY_FOLLOW_SPEED,
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
        if sweep_sentry_vs_wall(pos, &final_pos, wall) {
            collides = true;
            break;
        }
    }

    if !collides {
        for ramp in ramps {
            if sweep_sentry_vs_ramp_footprint(pos, &final_pos, ramp) {
                collides = true;
                break;
            }
        }
    }

    if collides {
        final_pos = slide_sentry_along_obstacles(walls, ramps, pos, desired_vel.x, desired_vel.z, delta);
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
            ServerMessage::Sentry(SSentry {
                id: *sentry_id,
                sentry: Sentry { pos: *pos, vel: *vel },
            }),
        );
    }
}

// ============================================================================
// Sentry-Player Collision System
// ============================================================================

// Check for sentry-player collisions and apply stun
pub fn sentry_player_collision_system(
    mut sentries: ResMut<SentryMap>,
    mut players: ResMut<PlayerMap>,
    sentry_query: Query<(&SentryId, &Position), With<SentryMarker>>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
) {
    // Collect sentry positions
    let sentry_positions: Vec<(SentryId, Position)> = sentry_query.iter().map(|(id, pos)| (*id, *pos)).collect();

    // Collect player collisions first
    let mut player_hits: Vec<(PlayerId, SentryId)> = Vec::new();

    for (player_id, player_position) in &player_query {
        let Some(player_info) = players.0.get(player_id) else {
            continue;
        };

        // Skip if already stunned
        if player_info.stun_timer > 0.0 {
            continue;
        }

        // Skip if player has hunt power-up
        if ALWAYS_SENTRY_HUNT || player_info.sentry_hunt_power_up_timer > 0.0 {
            continue;
        }

        // Check collision with any sentry
        for (sentry_id, sentry_pos) in &sentry_positions {
            let Some(sentry_info) = sentries.0.get(sentry_id) else {
                continue;
            };

            if sentry_info.mode != SentryMode::Target {
                continue; // Skip stunning if sentry is not targeting
            }

            if sentry_info.follow_target != Some(*player_id) {
                continue; // Sentry is targeting someone else
            }

            if overlap_sentry_vs_player(sentry_pos, player_position) {
                player_hits.push((*player_id, *sentry_id));
                break; // Only one hit per frame
            }
        }
    }

    // Apply stun and broadcast
    for (player_id, sentry_id) in player_hits {
        if let Some(player_info) = players.0.get_mut(&player_id) {
            player_info.stun_timer = SENTRY_STUN_DURATION;
            player_info.hits -= SENTRY_HIT_PENALTY;

            let status_msg = SPlayerStatus {
                id: player_id,
                speed_power_up: ALWAYS_SPEED || player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: ALWAYS_MULTI_SHOT || player_info.multi_shot_power_up_timer > 0.0,
                phasing_power_up: ALWAYS_PHASING || player_info.phasing_power_up_timer > 0.0,
                sentry_hunt_power_up: ALWAYS_SENTRY_HUNT || player_info.sentry_hunt_power_up_timer > 0.0,
                stunned: true,
            };

            // Send sentry hit message only to the hit player for sound effect
            let _ = player_info
                .channel
                .send(ServerToClient::Send(ServerMessage::SentryHit(SSentryHit {})));

            broadcast_to_all(&players, ServerMessage::PlayerStatus(status_msg));
        }

        // Put sentry into pre-patrol mode after hitting a player (will return to grid center)
        if let Some(sentry_info) = sentries.0.get_mut(&sentry_id) {
            sentry_info.mode = SentryMode::PrePatrol;
            sentry_info.mode_timer = 0.0;
        }
    }
}
