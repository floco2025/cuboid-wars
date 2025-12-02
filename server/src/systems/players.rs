use bevy::prelude::*;

use crate::resources::{GridConfig, PlayerMap};
use common::{
    collision::{calculate_wall_slide, check_player_player_collision, check_player_wall_collision},
    constants::SPEED_POWER_UP_MULTIPLIER,
    protocol::{PlayerId, Position, Speed},
};

#[derive(Copy, Clone)]
struct PlannedMove {
    entity: Entity,
    target: Position,
}

fn speed_multiplier(players: &PlayerMap, player_id: PlayerId) -> f32 {
    players
        .0
        .get(&player_id)
        .and_then(|info| (info.speed_power_up_timer > 0.0).then_some(SPEED_POWER_UP_MULTIPLIER))
        .unwrap_or(1.0)
}

fn overlaps_other_player(candidate: &PlannedMove, planned_moves: &[PlannedMove]) -> bool {
    planned_moves.iter().any(|other| {
        other.entity != candidate.entity && check_player_player_collision(&candidate.target, &other.target)
    })
}

// ============================================================================
// Player Movement System
// ============================================================================

pub fn player_movement_system(
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

        let target = if walls.iter().any(|wall| check_player_wall_collision(&new_pos, wall)) {
            calculate_wall_slide(walls, pos, &new_pos, velocity.x, velocity.z, delta)
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
