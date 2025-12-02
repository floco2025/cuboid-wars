use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    constants::*,
    map::cell_center,
    resources::{GhostInfo, GhostMap, GridCell, GridConfig, PlayerMap},
};
use common::{
    collision::check_ghost_wall_collision,
    constants::*,
    protocol::{Ghost, GhostId, Position, SGhost, ServerMessage, Velocity},
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
    const ALL: [Self; 4] = [
        Self::East,
        Self::North,
        Self::West,
        Self::South,
    ];

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
        let pos = cell_center(grid_x, grid_z);

        // Random initial velocity direction (only horizontal or vertical)
        let direction = pick_direction(&mut rng, &GridDirection::ALL).unwrap_or(GridDirection::East);
        let vel = direction.to_velocity();

        let ghost_id = GhostId(i);
        let entity = commands.spawn((ghost_id, pos, vel)).id();

        ghosts.0.insert(ghost_id, GhostInfo { entity });
    }
}

// ============================================================================
// Ghosts Movement System
// ============================================================================

pub fn ghosts_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut ghost_query: Query<(&GhostId, &mut Position, &mut Velocity)>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    for (ghost_id, mut pos, mut vel) in &mut ghost_query {
        // Calculate which grid cell we're in
        let grid_x = ((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
        let grid_z = ((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;

        // Check if ghost is within grid bounds
        if !(0..GRID_COLS).contains(&grid_x) || !(0..GRID_ROWS).contains(&grid_z) {
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
        let center = cell_center(grid_x, grid_z);

        // Check if we're at grid center (within small threshold)
        const CENTER_THRESHOLD: f32 = 0.1;
        let at_center_x = (pos.x - center.x).abs() < CENTER_THRESHOLD;
        let at_center_z = (pos.z - center.z).abs() < CENTER_THRESHOLD;
        let at_intersection = at_center_x && at_center_z;

        if at_intersection {
            let cell = &grid_config.grid[grid_z as usize][grid_x as usize];
            let valid_directions = valid_directions(*cell);
            let mut direction_changed = false;

            if let Some(current_direction) = direction_from_velocity(&vel)
                && current_direction.is_blocked(*cell) {
                    let forward_options = forward_directions(&valid_directions, current_direction);
                    if forward_options.is_empty() {
                        let new_direction = valid_directions.first().copied().expect("no valid direction");
                        *vel = new_direction.to_velocity();
                        direction_changed = true;
                    } else if let Some(new_direction) = pick_direction(&mut rng, &forward_options) {
                        *vel = new_direction.to_velocity();
                        direction_changed = true;
                    }
                }

            if rng.random_bool(GHOST_RANDOM_TURN_PROBABILITY) && !valid_directions.is_empty()
                && let Some(new_direction) = pick_direction(&mut rng, &valid_directions) {
                    *vel = new_direction.to_velocity();
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
            pos.z = center.z;
        } else if vel.z.abs() > 0.0 && vel.x.abs() < 0.01 {
            // Moving vertically - snap X to grid center
            pos.x = center.x;
        }
    }
}
