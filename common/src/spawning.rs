use crate::{
    collision::{check_player_ramp_edge_sweep, check_player_wall_sweep},
    constants::*,
    protocol::{Position, Ramp, Wall},
};
use bevy_math::Vec3;

// ============================================================================
// Projectile Spawning
// ============================================================================

// Information needed to spawn a single projectile
#[derive(Debug, Clone)]
pub struct ProjectileSpawnInfo {
    pub position: Position,
    pub direction: f32,
    pub reflects: bool,
}

// Calculate valid projectile spawn positions for a shot
//
// Returns a list of projectiles that should be spawned, excluding any that would
// be blocked by walls or ramp edges.
#[must_use]
pub fn calculate_projectile_spawns(
    shooter_pos: &Position,
    face_dir: f32,
    has_multi_shot: bool,
    has_reflect: bool,
    walls: &[Wall],
    ramps: &[Ramp],
) -> Vec<ProjectileSpawnInfo> {
    let mut spawns = Vec::new();

    // Determine number of shots
    let num_shots = if has_multi_shot {
        POWER_UP_MULTI_SHOT_MULTIPLER
    } else {
        1
    };

    // Spawn projectiles in an arc
    let angle_step = POWER_UP_MULTI_SHOT_ANGLE.to_radians();
    let start_offset = -(num_shots - 1) as f32 * angle_step / 2.0;

    for i in 0..num_shots {
        let angle_offset = (i as f32).mul_add(angle_step, start_offset);
        let shot_dir = face_dir + angle_offset;
        // Spawn projectile at constant height offset from player's feet position
        let spawn_pos = Vec3::new(
            shot_dir.sin().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.x),
            shooter_pos.y + PROJECTILE_SPAWN_HEIGHT,
            shot_dir.cos().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.z),
        );

        // Check if the path from player to spawn position crosses through a wall
        let spawn_position = Position {
            x: spawn_pos.x,
            y: spawn_pos.y,
            z: spawn_pos.z,
        };

        let is_spawn_blocked = walls
            .iter()
            .any(|wall| check_player_wall_sweep(shooter_pos, &spawn_position, wall))
            || ramps
                .iter()
                .any(|ramp| check_player_ramp_edge_sweep(shooter_pos, &spawn_position, ramp));

        // Skip this projectile if the spawn path is blocked by a wall
        if is_spawn_blocked {
            continue;
        }

        spawns.push(ProjectileSpawnInfo {
            position: spawn_position,
            direction: shot_dir,
            reflects: has_reflect,
        });
    }

    spawns
}
