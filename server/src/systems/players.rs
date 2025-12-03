use bevy::prelude::*;

use crate::resources::{GridConfig, PlayerMap};
use common::{
    collision::{calculate_wall_slide, check_player_player_overlap, check_player_wall_sweep},
    constants::POWER_UP_SPEED_MULTIPLIER,
    protocol::{PlayerId, Position, Speed, SPlayerStatus, ServerMessage},
};

use super::network::broadcast_to_all;

// ============================================================================
// Helper Functions
// ============================================================================

#[derive(Copy, Clone)]
struct PlannedMove {
    entity: Entity,
    target: Position,
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
    let walls = &grid_config.walls;

    // Pass 1: Calculate all intended new positions (after wall collision check with sliding)
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, pos, speed, player_id) in query.iter() {
        // Check if player is stunned
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        
        if is_stunned {
            // Stunned players cannot move
            planned_moves.push(PlannedMove { entity, target: *pos });
            continue;
        }

        let multiplier = speed_multiplier(&players, *player_id);
        let mut velocity = speed.to_velocity();
        velocity.x *= multiplier;
        velocity.z *= multiplier;

        if velocity.x == 0.0 && velocity.z == 0.0 {
            planned_moves.push(PlannedMove { entity, target: *pos });
            continue;
        }

        let new_pos = Position {
            x: velocity.x.mul_add(delta, pos.x),
            y: pos.y,
            z: velocity.z.mul_add(delta, pos.z),
        };

        let target = if walls.iter().any(|wall| check_player_wall_sweep(pos, &new_pos, wall)) {
            calculate_wall_slide(walls, pos, velocity.x, velocity.z, delta)
        } else {
            new_pos
        };

        planned_moves.push(PlannedMove { entity, target });
    }

    // Pass 2: Check player-player collisions and apply positions
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
        let old_stunned = player_info.stun_timer > 0.0;

        // Decrease power-up timers
        player_info.speed_power_up_timer = (player_info.speed_power_up_timer - delta).max(0.0);
        player_info.multi_shot_power_up_timer = (player_info.multi_shot_power_up_timer - delta).max(0.0);
        player_info.reflect_power_up_timer = (player_info.reflect_power_up_timer - delta).max(0.0);
        player_info.stun_timer = (player_info.stun_timer - delta).max(0.0);

        let new_speed = player_info.speed_power_up_timer > 0.0;
        let new_multi_shot = player_info.multi_shot_power_up_timer > 0.0;
        let new_reflect = player_info.reflect_power_up_timer > 0.0;
        let new_stunned = player_info.stun_timer > 0.0;

        // Track changes to broadcast
        if old_speed != new_speed || old_multi_shot != new_multi_shot || old_reflect != new_reflect || old_stunned != new_stunned {
            status_messages.push(SPlayerStatus {
                id: *player_id,
                speed_power_up: new_speed,
                multi_shot_power_up: new_multi_shot,
                reflect_power_up: new_reflect,
                stunned: new_stunned,
            });
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
