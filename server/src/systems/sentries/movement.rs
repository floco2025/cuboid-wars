use super::navigation::{GridDirection, ahead_directions, direction_from_velocity, pick_direction, valid_directions};
use crate::{
    constants::*,
    map::cell_center,
    resources::{GridConfig, PlayerMap, SentryGrid, SentryInfo, SentryMode},
    systems::network::broadcast_to_all,
};
use common::{
    collision::{slide_sentry_along_obstacles, sweep_player_vs_wall},
    constants::*,
    protocol::*,
};

// ============================================================================
// AI Helper Functions
// ============================================================================

// Find the first moving player visible from sentry's position using line-of-sight check
#[must_use]
pub fn find_visible_moving_player(
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

        let dx = player_pos.x - sentry_pos.x;
        let dz = player_pos.z - sentry_pos.z;
        let distance_sq = dx.mul_add(dx, dz * dz);

        if distance_sq > SENTRY_VISION_RANGE * SENTRY_VISION_RANGE {
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

// ============================================================================
// Movement Modes
// ============================================================================

// Pre-patrol mode movement - navigates to grid center before entering patrol
pub fn pre_patrol_movement(
    sentry_id: &SentryId,
    pos: &mut Position,
    vel: &mut Velocity,
    face_dir: &mut f32,
    sentry_info: &mut SentryInfo,
    players: &PlayerMap,
    sentry_grid: &mut SentryGrid,
    delta: f32,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    // If the destination cell is already occupied by another sentry, stop immediately
    let field = &mut sentry_grid.0[grid_z as usize][grid_x as usize];
    if field.is_some() && field.expect("should be some") != *sentry_id {
        *vel = Velocity { x: 0.0, y: 0.0, z: 0.0 };
        *face_dir = 0.0;
        return;
    }

    *field = Some(*sentry_id);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    if at_intersection {
        *vel = Velocity { x: 0.0, y: 0.0, z: 0.0 };
        *face_dir = 0.0;

        sentry_info.mode = SentryMode::Patrol;
        sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION;
        sentry_info.at_intersection = true;

        broadcast_to_all(
            players,
            ServerMessage::Sentry(SSentry {
                id: *sentry_id,
                sentry: Sentry {
                    pos: *pos,
                    vel: *vel,
                },
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
        *face_dir = vel.x.atan2(vel.z);

        if vel_changed {
            broadcast_to_all(
                players,
                ServerMessage::Sentry(SSentry {
                    id: *sentry_id,
                    sentry: Sentry {
                        pos: *pos,
                        vel: *vel,
                    },
                }),
            );
        }

        pos.x += vel.x * delta;
        pos.z += vel.z * delta;
    }
}

// Patrol mode movement - follows grid lines
pub fn patrol_movement(
    sentry_id: &SentryId,
    pos: &mut Position,
    vel: &mut Velocity,
    face_dir: &mut f32,
    sentry_info: &mut SentryInfo,
    grid_config: &GridConfig,
    players: &PlayerMap,
    sentry_grid: &mut SentryGrid,
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);

    let field = &mut sentry_grid.0[grid_z as usize][grid_x as usize];
    assert!(field.is_some());
    assert!(field.expect("should be some") == *sentry_id);

    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    let mut current_direction = direction_from_velocity(vel);
    let just_arrived = at_intersection && !sentry_info.at_intersection;
    sentry_info.at_intersection = at_intersection;

    if just_arrived || current_direction == GridDirection::None {
        if current_direction != GridDirection::None {
            let (prev_grid_x, prev_grid_z) = match current_direction {
                GridDirection::North => (grid_x, grid_z + 1),
                GridDirection::South => (grid_x, grid_z - 1),
                GridDirection::East => (grid_x - 1, grid_z),
                GridDirection::West => (grid_x + 1, grid_z),
                GridDirection::None => unreachable!("none case guarded above"),
            };
            assert!((0..GRID_COLS).contains(&prev_grid_x));
            assert!((0..GRID_ROWS).contains(&prev_grid_z));
            let field = &mut sentry_grid.0[prev_grid_z as usize][prev_grid_x as usize];
            assert!(field.is_some());
            assert!(field.expect("should be some") == *sentry_id);
            *field = None;
        }

        let valid_directions = valid_directions(grid_config, grid_x, grid_z, &sentry_grid.0, *sentry_id);
        let mut direction_changed = false;

        if valid_directions.is_empty() {
            if current_direction != GridDirection::None {
                *vel = Velocity { x: 0.0, y: 0.0, z: 0.0 };
                direction_changed = true;
            }
        } else {
            if current_direction.is_blocked(grid_config, grid_x, grid_z, &sentry_grid.0, *sentry_id) {
                let ahead_directions = ahead_directions(&valid_directions, current_direction);
                if ahead_directions.is_empty() {
                    let new_direction = valid_directions.first().copied().expect("no valid direction");
                    *vel = new_direction.to_velocity();
                    direction_changed = true;
                } else if let Some(new_direction) = pick_direction(rng, &ahead_directions) {
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
        }

        if direction_changed {
            *face_dir = vel.x.atan2(vel.z);

            broadcast_to_all(
                players,
                ServerMessage::Sentry(SSentry {
                    id: *sentry_id,
                    sentry: Sentry {
                        pos: *pos,
                        vel: *vel,
                    },
                }),
            );
        }

        current_direction = direction_from_velocity(vel);
        if current_direction != GridDirection::None {
            let (next_grid_x, next_grid_z) = match current_direction {
                GridDirection::North => (grid_x, grid_z - 1),
                GridDirection::South => (grid_x, grid_z + 1),
                GridDirection::East => (grid_x + 1, grid_z),
                GridDirection::West => (grid_x - 1, grid_z),
                GridDirection::None => unreachable!("none case guarded above"),
            };
            assert!((0..GRID_COLS).contains(&next_grid_x));
            assert!((0..GRID_ROWS).contains(&next_grid_z));
            let field = &mut sentry_grid.0[next_grid_z as usize][next_grid_x as usize];
            assert!(field.is_none() || field.expect("should be some") == *sentry_id);
            *field = Some(*sentry_id);
        }
    }

    pos.x += vel.x * delta;
    pos.z += vel.z * delta;

    // Incrementally adjust position toward grid line based on current direction
    match current_direction {
        GridDirection::None => {
            // No direction, no adjustment needed
        }
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

// Target mode movement - moves toward target player with wall sliding
// If the target player has sentry hunt power-up, reverses direction to flee
pub fn target_movement(
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

    // Apply sliding movement
    let final_pos = slide_sentry_along_obstacles(walls, ramps, pos, desired_vel.x, desired_vel.z, delta);

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
                sentry: Sentry {
                    pos: *pos,
                    vel: *vel,
                },
            }),
        );
    }
}
