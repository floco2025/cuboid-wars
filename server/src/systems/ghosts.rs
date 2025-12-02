use bevy::prelude::*;
use rand::Rng as _;

use crate::{constants::*, resources::{GhostInfo, GhostMap, GridCell, GridConfig, PlayerMap}};
use common::{
    collision::check_ghost_wall_collision,
    constants::*,
    protocol::{Ghost, GhostId, Position, SGhost, ServerMessage, Velocity},
};

use super::network::broadcast_to_all;

// ============================================================================
// Ghost Helper Functions
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
fn is_direction_blocked(cell: &GridCell, direction: i32) -> bool {
    match direction {
        0 => cell.has_east_wall,  // Right
        1 => cell.has_north_wall, // Up
        2 => cell.has_west_wall,  // Left
        3 => cell.has_south_wall, // Down
        _ => true,
    }
}

// Helper function to get all valid (non-blocked) directions
fn get_valid_directions(cell: &GridCell) -> Vec<i32> {
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

// ============================================================================
// Ghost Systems
// ============================================================================

// System to spawn initial ghosts on server startup
pub fn ghost_spawn_system(
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

        ghosts.0.insert(ghost_id, GhostInfo { entity });
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
