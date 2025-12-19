use bevy::prelude::*;

use crate::{resources::PlayerMap, systems::network::broadcast_to_all};
use common::protocol::MapLayout;
use common::{
    collision::players::{slide_player_along_obstacles, sweep_player_vs_ramp_edges, sweep_player_vs_wall},
    constants::{ALWAYS_PHASING, ALWAYS_SPEED, POWER_UP_SPEED_MULTIPLIER, ROOF_HEIGHT},
    map::{close_to_roof, has_roof, height_on_ramp},
    markers::PlayerMarker,
    players::{PlannedMove, overlaps_other_player},
    protocol::{PlayerId, Position, SPlayerStatus, ServerMessage, Speed},
};

// ============================================================================
// Players Movement System
// ============================================================================

pub fn players_movement_system(
    time: Res<Time>,
    map_layout: Res<MapLayout>,
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
                collides: false,
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
                collides: false,
            });
            continue;
        }

        // Calculate intended position from velocity
        let mut target_pos = Position {
            x: velocity.x.mul_add(delta, pos.x),
            y: pos.y, // Keep current Y for collision detection
            z: velocity.z.mul_add(delta, pos.z),
        };

        // Check collision and calculate target (with sliding if collision)
        let walls_to_check = if close_to_roof(pos.y) {
            &map_layout.roof_walls
        } else {
            let has_phasing = ALWAYS_PHASING
                || players
                    .0
                    .get(player_id)
                    .is_some_and(|info| info.phasing_power_up_timer > 0.0);
            if has_phasing {
                &map_layout.boundary_walls
            } else {
                &map_layout.lower_walls
            }
        };

        // Check collision and calculate target (with sliding if collision)
        let mut collides = false;

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

        let target_height_on_ramp = height_on_ramp(&map_layout.ramps, target_pos.x, target_pos.z);
        let target_has_roof = has_roof(&map_layout.roofs, target_pos.x, target_pos.z);

        if target_height_on_ramp > 0.0 {
            target_pos.y = target_height_on_ramp;
        } else if target_has_roof && close_to_roof(pos.y) {
            target_pos.y = ROOF_HEIGHT;
        } else {
            target_pos.y = 0.0;
        }

        planned_moves.push(PlannedMove {
            entity,
            start: *pos,
            target: target_pos,
            collides,
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
