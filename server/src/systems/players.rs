use bevy::prelude::*;

use crate::{resources::PlayerMap, systems::network::broadcast_to_all};
use common::protocol::MapLayout;
use common::{
    collision::players::{slide_player_along_obstacles, sweep_player_vs_ramp_edges, sweep_player_vs_wall},
    constants::{ALWAYS_PHASING, ALWAYS_SPEED, POWER_UP_SPEED_MULTIPLIER, ROOF_HEIGHT},
    markers::PlayerMarker,
    players::{PlannedMove, overlaps_other_player},
    protocol::{PlayerId, Position, SPlayerStatus, ServerMessage, Speed, Wall},
    ramps::{calculate_height_at_position, is_on_roof},
};

// ============================================================================
// Players Movement System
// ============================================================================

pub fn players_movement_system(
    time: Res<Time>,
    grid_config: Res<MapLayout>,
    players: Res<PlayerMap>,
    mut query: Query<(Entity, &mut Position, &Speed, &PlayerId), With<PlayerMarker>>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position, then apply wall collision logic
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, pos, speed, player_id) in query.iter() {
        // Check if player is stunned
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);

        if is_stunned {
            // Stunned players cannot move
            planned_moves.push(PlannedMove {
                entity,
                start: *pos,
                target: *pos,
                hits_wall: false,
            });
            continue;
        }

        // Calculate intended position from velocity
        let multiplier = players
            .0
            .get(player_id)
            .and_then(|info| (ALWAYS_SPEED || info.speed_power_up_timer > 0.0).then_some(POWER_UP_SPEED_MULTIPLIER))
            .unwrap_or(1.0);
        let mut velocity = speed.to_velocity();
        velocity.x *= multiplier;
        velocity.z *= multiplier;

        let abs_velocity = velocity.x.hypot(velocity.z);
        let is_standing_still = abs_velocity < f32::EPSILON;

        if is_standing_still {
            planned_moves.push(PlannedMove {
                entity,
                start: *pos,
                target: *pos,
                hits_wall: false,
            });
            continue;
        }

        // Calculate new X/Z position but keep Y for collision detection
        let new_pos_xz = Position {
            x: velocity.x.mul_add(delta, pos.x),
            y: pos.y, // Keep current Y for collision detection
            z: velocity.z.mul_add(delta, pos.z),
        };

        // Wall collision - Select walls based on phasing power-up and height
        let has_phasing = ALWAYS_PHASING
            || players
                .0
                .get(player_id)
                .is_some_and(|info| info.phasing_power_up_timer > 0.0);

        let mut walls_to_check = Vec::new();

        if is_on_roof(pos.y) {
            // On roof: only roof edge walls (which have openings at ramp connections)
            walls_to_check.extend_from_slice(&grid_config.roof_edge_walls);
        } else {
            // On ground: all walls (or just boundary if phasing) plus ramp walls
            let base_walls: &[Wall] = if has_phasing {
                &grid_config.boundary_walls
            } else {
                &grid_config.lower_walls
            };
            walls_to_check.extend_from_slice(base_walls);
        }

        // Check wall/ramp collision and calculate target (with sliding if hit)
        let mut hits_wall = false;

        for wall in &walls_to_check {
            if sweep_player_vs_wall(pos, &new_pos_xz, wall) {
                hits_wall = true;
                break;
            }
        }

        if !hits_wall {
            for ramp in &grid_config.ramps {
                if sweep_player_vs_ramp_edges(pos, &new_pos_xz, ramp) {
                    hits_wall = true;
                    break;
                }
            }
        }

        let (target_xz, hits_wall) = if hits_wall {
            (
                slide_player_along_obstacles(&walls_to_check, &grid_config.ramps, pos, velocity.x, velocity.z, delta),
                true,
            )
        } else {
            (new_pos_xz, false)
        };

        // Now calculate final Y based on the collision-adjusted X/Z position
        let final_y = {
            let ramp_height = calculate_height_at_position(&grid_config.ramps, target_xz.x, target_xz.z);
            if ramp_height > 0.0 {
                ramp_height
            } else if grid_config.is_position_on_roof(target_xz.x, target_xz.z) && is_on_roof(pos.y) {
                // Only stay on roof if already at roof height
                ROOF_HEIGHT
            } else {
                0.0
            }
        };

        let target = Position {
            x: target_xz.x,
            y: final_y,
            z: target_xz.z,
        };

        planned_moves.push(PlannedMove {
            entity,
            start: *pos,
            target,
            hits_wall,
        });
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        if overlaps_other_player(planned_move, &planned_moves) {
            continue;
        }

        if let Ok((_, mut pos, _, _)) = query.get_mut(planned_move.entity) {
            *pos = planned_move.target;
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
        let old_speed = player_info.speed_power_up_timer > 0.0;
        let old_multi_shot = player_info.multi_shot_power_up_timer > 0.0;
        let old_reflect = player_info.reflect_power_up_timer > 0.0;
        let old_phasing = player_info.phasing_power_up_timer > 0.0;
        let old_ghost_hunt = player_info.ghost_hunt_power_up_timer > 0.0;
        let old_stunned = player_info.stun_timer > 0.0;

        // Decrease power-up timers
        player_info.speed_power_up_timer = (player_info.speed_power_up_timer - delta).max(0.0);
        player_info.multi_shot_power_up_timer = (player_info.multi_shot_power_up_timer - delta).max(0.0);
        player_info.reflect_power_up_timer = (player_info.reflect_power_up_timer - delta).max(0.0);
        player_info.phasing_power_up_timer = (player_info.phasing_power_up_timer - delta).max(0.0);
        player_info.ghost_hunt_power_up_timer = (player_info.ghost_hunt_power_up_timer - delta).max(0.0);
        player_info.stun_timer = (player_info.stun_timer - delta).max(0.0);

        let new_speed = player_info.speed_power_up_timer > 0.0;
        let new_multi_shot = player_info.multi_shot_power_up_timer > 0.0;
        let new_reflect = player_info.reflect_power_up_timer > 0.0;
        let new_phasing = player_info.phasing_power_up_timer > 0.0;
        let new_ghost_hunt = player_info.ghost_hunt_power_up_timer > 0.0;
        let new_stunned = player_info.stun_timer > 0.0;

        // Track changes to broadcast
        if old_speed != new_speed
            || old_multi_shot != new_multi_shot
            || old_reflect != new_reflect
            || old_phasing != new_phasing
            || old_ghost_hunt != new_ghost_hunt
            || old_stunned != new_stunned
        {
            status_messages.push(SPlayerStatus {
                id: *player_id,
                speed_power_up: new_speed,
                multi_shot_power_up: new_multi_shot,
                reflect_power_up: new_reflect,
                phasing_power_up: new_phasing,
                ghost_hunt_power_up: new_ghost_hunt,
                stunned: new_stunned,
            });
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
