use bevy::prelude::*;

use crate::resources::{GridConfig, PlayerMap};
use common::{
    collision::{calculate_wall_slide, check_player_player_overlap, check_player_wall_sweep},
    constants::POWER_UP_SPEED_MULTIPLIER,
    protocol::{PlayerId, Position, SPlayerStatus, ServerMessage, Speed, Wall},
};

use super::network::broadcast_to_all;

// ============================================================================
// Helper Functions
// ============================================================================

#[derive(Copy, Clone)]
struct PlannedMove {
    entity: Entity,
    target: Position,
    #[allow(dead_code)] // Server doesn't use this for feedback, but kept for consistency with client
    hits_wall: bool,
}

fn speed_multiplier(players: &PlayerMap, player_id: PlayerId) -> f32 {
    players
        .0
        .get(&player_id)
        .and_then(|info| (info.speed_power_up_timer > 0.0).then_some(POWER_UP_SPEED_MULTIPLIER))
        .unwrap_or(1.0)
}

fn overlaps_other_player(candidate: &PlannedMove, planned_moves: &[PlannedMove]) -> bool {
    planned_moves
        .iter()
        .any(|other| other.entity != candidate.entity && check_player_player_overlap(&candidate.target, &other.target))
}

// ============================================================================
// Players Movement System
// ============================================================================

pub fn players_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut query: Query<(Entity, &mut Position, &Speed, &PlayerId)>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position, then apply wall collision logic
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, pos, speed, player_id) in query.iter() {
        // Check if player is stunned
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);

        if is_stunned {
            // Stunned players cannot move
            planned_moves.push(PlannedMove { entity, target: *pos, hits_wall: false });
            continue;
        }

        // Calculate intended position from velocity
        let multiplier = speed_multiplier(&players, *player_id);
        let mut velocity = speed.to_velocity();
        velocity.x *= multiplier;
        velocity.z *= multiplier;

        let abs_velocity = velocity.x.hypot(velocity.z);
        let is_standing_still = abs_velocity < f32::EPSILON;

        if is_standing_still {
            planned_moves.push(PlannedMove { entity, target: *pos, hits_wall: false });
            continue;
        }

        let new_pos = Position {
            x: velocity.x.mul_add(delta, pos.x),
            y: pos.y,
            z: velocity.z.mul_add(delta, pos.z),
        };

        // Wall collision - Select walls based on phasing power-up
        let has_phasing = players
            .0
            .get(player_id)
            .is_some_and(|info| info.phasing_power_up_timer > 0.0);

        let walls_to_check: &[Wall] = if has_phasing {
            &grid_config.boundary_walls
        } else {
            &grid_config.all_walls
        };

        // Check wall collision and calculate target (with sliding if hit)
        let (target, hits_wall) = if walls_to_check.iter().any(|wall| check_player_wall_sweep(pos, &new_pos, wall)) {
            (calculate_wall_slide(walls_to_check, pos, velocity.x, velocity.z, delta), true)
        } else {
            (new_pos, false)
        };

        planned_moves.push(PlannedMove { entity, target, hits_wall });
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
        let old_stunned = player_info.stun_timer > 0.0;

        // Decrease power-up timers
        player_info.speed_power_up_timer = (player_info.speed_power_up_timer - delta).max(0.0);
        player_info.multi_shot_power_up_timer = (player_info.multi_shot_power_up_timer - delta).max(0.0);
        player_info.reflect_power_up_timer = (player_info.reflect_power_up_timer - delta).max(0.0);
        player_info.phasing_power_up_timer = (player_info.phasing_power_up_timer - delta).max(0.0);
        player_info.stun_timer = (player_info.stun_timer - delta).max(0.0);

        let new_speed = player_info.speed_power_up_timer > 0.0;
        let new_multi_shot = player_info.multi_shot_power_up_timer > 0.0;
        let new_reflect = player_info.reflect_power_up_timer > 0.0;
        let new_phasing = player_info.phasing_power_up_timer > 0.0;
        let new_stunned = player_info.stun_timer > 0.0;

        // Track changes to broadcast
        if old_speed != new_speed
            || old_multi_shot != new_multi_shot
            || old_reflect != new_reflect
            || old_phasing != new_phasing
            || old_stunned != new_stunned
        {
            status_messages.push(SPlayerStatus {
                id: *player_id,
                speed_power_up: new_speed,
                multi_shot_power_up: new_multi_shot,
                reflect_power_up: new_reflect,
                phasing_power_up: new_phasing,
                stunned: new_stunned,
            });
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
