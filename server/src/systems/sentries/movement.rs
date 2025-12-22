use super::navigation::{
    GridDirection, direction_from_velocity, direction_leads_to_ramp, forward_directions, pick_direction,
    valid_directions,
};
use crate::{
    constants::*,
    map::cell_center,
    resources::{GridConfig, PlayerMap, SentryInfo, SentryMode},
    systems::network::broadcast_to_all,
};
use common::{
    collision::{
        players::sweep_player_vs_wall,
        sentries::{slide_sentry_along_obstacles, sweep_sentry_vs_ramp_footprint, sweep_sentry_vs_wall},
    },
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
    delta: f32,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    if at_intersection {
        // We've reached the grid center - transition to patrol with zero velocity
        *vel = Velocity { x: 0.0, y: 0.0, z: 0.0 };
        *face_dir = 0.0;
        sentry_info.mode = SentryMode::Patrol;
        sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION; // Set cooldown before can detect players again
        sentry_info.at_intersection = true;

        broadcast_to_all(
            players,
            ServerMessage::Sentry(SSentry {
                id: *sentry_id,
                sentry: Sentry {
                    pos: *pos,
                    vel: *vel,
                    face_dir: vel.x.atan2(vel.z),
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
                        face_dir: vel.x.atan2(vel.z),
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
    delta: f32,
    rng: &mut impl rand::Rng,
) {
    let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
    let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);
    let center = cell_center(grid_x, grid_z);

    let at_center_x = (pos.x - center.x).abs() < SENTRY_CENTER_THRESHOLD;
    let at_center_z = (pos.z - center.z).abs() < SENTRY_CENTER_THRESHOLD;
    let at_intersection = at_center_x && at_center_z;

    let mut current_direction = direction_from_velocity(vel);
    let just_arrived = at_intersection && (!sentry_info.at_intersection || current_direction == GridDirection::None);
    sentry_info.at_intersection = at_intersection;

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
            *face_dir = vel.x.atan2(vel.z);
            broadcast_to_all(
                players,
                ServerMessage::Sentry(SSentry {
                    id: *sentry_id,
                    sentry: Sentry {
                        pos: *pos,
                        vel: *vel,
                        face_dir: *face_dir,
                    },
                }),
            );

            current_direction = direction_from_velocity(vel);
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

// Follow mode movement - moves toward target player with wall sliding
// If the target player has sentry hunt power-up, reverses direction to flee
pub fn follow_movement(
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
                sentry: Sentry {
                    pos: *pos,
                    vel: *vel,
                    face_dir: vel.x.atan2(vel.z),
                },
            }),
        );
    }
}
