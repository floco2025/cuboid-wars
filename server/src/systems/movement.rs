use bevy::prelude::*;

use crate::resources::{GridConfig, PlayerMap};
use common::{
    collision::{calculate_wall_slide, check_player_player_collision, check_player_wall_collision},
    constants::SPEED_POWER_UP_MULTIPLIER,
    protocol::{PlayerId, Position, Speed},
};

// ============================================================================
// Movement System (Server with Wall Collision)
// ============================================================================

pub fn server_movement_system(
    time: Res<Time>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut query: Query<(Entity, &mut Position, &Speed, &PlayerId)>,
) {
    let delta = time.delta_secs();

    // Pass 1: Calculate all intended new positions (after wall collision check with sliding)
    let mut intended_positions: Vec<(Entity, Position)> = Vec::new();

    for (entity, pos, speed, player_id) in query.iter() {
        // Convert Speed to Velocity
        let mut velocity = speed.to_velocity();

        // Apply speed power-up multiplier if active
        if let Some(player_info) = players.0.get(player_id) {
            if player_info.speed_power_up_timer > 0.0 {
                velocity.x *= SPEED_POWER_UP_MULTIPLIER;
                velocity.z *= SPEED_POWER_UP_MULTIPLIER;
            }
        }

        let speed = velocity.x.hypot(velocity.z);

        if speed > 0.0 {
            // Calculate new position
            let new_pos = Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z),
            };

            // Check if new position collides with any wall
            let collides_with_wall = grid_config
                .walls
                .iter()
                .any(|wall| check_player_wall_collision(&new_pos, wall));

            // Store intended position (slide along wall if collision, new otherwise)
            if collides_with_wall {
                let slide_pos = calculate_wall_slide(
                    &grid_config.walls,
                    pos,
                    &new_pos,
                    velocity.x,
                    velocity.z,
                    delta,
                );
                intended_positions.push((entity, slide_pos));
            } else {
                intended_positions.push((entity, new_pos));
            }
        } else {
            // Not moving, keep current position
            intended_positions.push((entity, *pos));
        }
    }

    // Pass 2: Check player-player collisions and apply positions
    for (entity, intended_pos) in &intended_positions {
        // Check if intended position collides with any other player's intended position
        let collides_with_player = intended_positions.iter().any(|(other_entity, other_intended_pos)| {
            *other_entity != *entity && check_player_player_collision(intended_pos, other_intended_pos)
        });

        if !collides_with_player {
            // No collision, apply intended position
            if let Ok((_, mut pos, _, _)) = query.get_mut(*entity) {
                *pos = *intended_pos;
            }
        }
        // If collision, don't update position (stays at current)
    }
}
