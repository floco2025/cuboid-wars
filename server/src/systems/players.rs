use bevy::prelude::*;
use rand::{RngExt, rng, seq::IndexedRandom};

use super::network::broadcast_to_all;
use crate::resources::{GridConfig, PlayerInfo, PlayerMap};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE, PHYSICS_EPSILON, PLAYER_DEATH_Y, PLAYER_RESPAWN_INVULN_SECS},
    map::{compute_player_level, find_support_floor},
    markers::PlayerMarker,
    physics::{
        PlayerMotion, overlap_player_vs_wall, slide_player_along_obstacles, sweep_player_vs_ramp_edges,
        sweep_player_vs_wall,
    },
    players::{PlannedMove, overlaps_other_player},
    protocol::{MapLayout, MoveInput, PlayerId, Position, SDeath, ServerMessage, Wall},
};

// ============================================================================
// Players Movement System
// ============================================================================

pub fn players_movement_system(
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    players: Res<PlayerMap>,
    mut query: Query<(Entity, &mut Position, &mut PlayerMotion, &MoveInput, &PlayerId), With<PlayerMarker>>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position + vy, then apply wall collision logic
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, pos, motion, move_input, player_id) in query.iter() {
        // Check if player is stunned
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);

        // Compute horizontal velocity from input intent + speed power-up.
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_input.to_velocity_for_player(has_speed_power_up);
        let velocity_sq = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
        let is_standing_still = velocity_sq < PHYSICS_EPSILON * PHYSICS_EPSILON;
        let suppress_horizontal = is_stunned || is_standing_still;

        let mut target_pos = if suppress_horizontal {
            *pos
        } else {
            Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y, // Keep current Y for collision detection
                z: velocity.z.mul_add(delta, pos.z),
            }
        };

        // Check collision and calculate target (with sliding if collision)
        let player_level = compute_player_level(pos.y);
        let walls_to_check: &[Wall] = if player_level == 1 {
            &map_layout.roof_walls
        } else {
            let has_phasing = players.0.get(player_id).is_some_and(PlayerInfo::has_phasing);
            let is_stuck_in_wall = !has_phasing
                && map_layout
                    .interior_walls
                    .iter()
                    .any(|wall| overlap_player_vs_wall(pos, wall));

            if has_phasing || is_stuck_in_wall {
                &map_layout.boundary_walls
            } else {
                &map_layout.lower_walls
            }
        };

        let mut collides = false;
        if !suppress_horizontal {
            for wall in walls_to_check {
                if sweep_player_vs_wall(pos, &target_pos, wall) {
                    collides = true;
                    break;
                }
            }

            if !collides {
                for ramp in &map_layout.ramps {
                    if sweep_player_vs_ramp_edges(pos, &target_pos, ramp) {
                        collides = true;
                        break;
                    }
                }
            }

            if collides {
                target_pos =
                    slide_player_along_obstacles(walls_to_check, &map_layout.ramps, pos, velocity.x, velocity.z, delta);
            }
        }

        // Vertical integration: apply gravity, then either land on a support or keep falling.
        let mut next_motion = PlayerMotion {
            velocity: motion.velocity,
        };
        next_motion.apply_gravity(delta);
        next_motion.apply_terminal_velocity();

        let support = find_support_floor(&map_layout.floors, &map_layout.ramps, target_pos.x, target_pos.z, pos.y);
        let target_vy;
        if next_motion.velocity.y <= 0.0
            && let Some(s) = support
        {
            target_pos.y = s;
            target_vy = 0.0;
        } else {
            target_pos.y = next_motion.velocity.y.mul_add(delta, pos.y);
            target_vy = next_motion.velocity.y;
        }

        planned_moves.push(PlannedMove {
            entity,
            start: *pos,
            target: target_pos,
            target_vy,
            collides,
        });
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        if overlaps_other_player(planned_move, &planned_moves) {
            continue;
        }

        if let Ok((_, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) {
            *pos = planned_move.target;
            motion.velocity.y = planned_move.target_vy;
        }
    }
}

// ============================================================================
// Players Timer System
// ============================================================================

// System to count down player power-up and stun timers
pub fn players_timer_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in &mut players.0 {
        let old_status = player_info.status(*player_id);

        player_info.tick_timers(delta);

        let new_status = player_info.status(*player_id);

        if old_status != new_status {
            status_messages.push(new_status);
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

// ============================================================================
// Players Death System
// ============================================================================

// Detect players that have fallen below the death threshold and respawn them.
// Sets a brief stun timer on respawn so the player stays still as their
// "invulnerability" window. Broadcasts an `SDeath` message so clients can
// teleport the entity immediately rather than waiting for the next `SUpdate`.
pub fn players_death_system(
    mut players: ResMut<PlayerMap>,
    grid_config: Res<GridConfig>,
    mut player_query: Query<(Entity, &PlayerId, &mut Position, &mut PlayerMotion), With<PlayerMarker>>,
) {
    let dead: Vec<(Entity, PlayerId)> = player_query
        .iter()
        .filter_map(|(entity, id, pos, _)| (pos.y < PLAYER_DEATH_Y).then_some((entity, *id)))
        .collect();

    if dead.is_empty() {
        return;
    }

    // Snapshot all current positions so spawn-distance checks see a consistent view.
    let occupied_positions: Vec<Position> = player_query.iter().map(|(_, _, pos, _)| *pos).collect();

    for (entity, id) in dead {
        let respawn_pos = generate_player_spawn_position(&grid_config, &occupied_positions);

        if let Ok((_, _, mut pos, mut motion)) = player_query.get_mut(entity) {
            *pos = respawn_pos;
            motion.velocity = Vec3::ZERO;
        }

        if let Some(info) = players.0.get_mut(&id) {
            info.stun_timer = PLAYER_RESPAWN_INVULN_SECS;
        }

        info!("{:?} died and respawned at {:?}", id, respawn_pos);
        broadcast_to_all(&players, ServerMessage::Death(SDeath { id, respawn_pos }));
    }
}

// ============================================================================
// Spawn Position Helper
// ============================================================================

const SPAWN_MIN_DISTANCE: f32 = 10.0;
const SPAWN_MAX_ATTEMPTS: usize = 100;

// Pick a random spawn position in a non-ramp grid cell, at least
// `SPAWN_MIN_DISTANCE` away from every position in `occupied_positions`.
// Returns the field center if no valid placement is found in time.
#[must_use]
pub fn generate_player_spawn_position(grid_config: &GridConfig, occupied_positions: &[Position]) -> Position {
    let mut rng = rng();
    let grid_rows = grid_config.grid.len() as i32;
    let grid_cols = grid_config.grid[0].len() as i32;

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

    for _ in 0..SPAWN_MAX_ATTEMPTS {
        let &(row, col) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");
        let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        let spawn_range = GRID_SIZE * 0.5 / 2.0;

        let pos = Position {
            x: cell_center_x + rng.random_range(-spawn_range..=spawn_range),
            y: 0.0,
            z: cell_center_z + rng.random_range(-spawn_range..=spawn_range),
        };

        let too_close = occupied_positions.iter().any(|p| {
            let dx = pos.x - p.x;
            let dz = pos.z - p.z;
            dx.mul_add(dx, dz * dz) < SPAWN_MIN_DISTANCE * SPAWN_MIN_DISTANCE
        });

        if !too_close {
            return pos;
        }
    }

    warn!(
        "could not generate spawn position after {} attempts, spawning at center",
        SPAWN_MAX_ATTEMPTS
    );
    Position::default()
}
