use crate::{
    constants::*,
    map::height_on_ramp,
    physics::{sweep_player_vs_floor, sweep_point_vs_cuboid},
    protocol::{Floor, Position, Ramp, Wall},
};
use bevy_math::Vec3;

// ============================================================================
// Projectile Spawning
// ============================================================================

// Information needed to spawn a single projectile
#[derive(Debug, Clone)]
pub struct ProjectileSpawnInfo {
    pub position: Position,
    pub direction_yaw: f32,
    pub direction_pitch: f32,
}

// Calculate valid projectile spawn positions for a shot
//
// Returns a list of projectiles that should be spawned, excluding any that would
// be blocked by walls on the way from the muzzle to the spawn point.
#[must_use]
pub fn calculate_projectile_spawns(
    shooter_pos: &Position,
    face_dir: f32,
    face_pitch: f32,
    has_multi_shot: bool,
    walls: &[Wall],
    ramps: &[Ramp],
    floors: &[Floor],
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
        let shot_yaw = face_dir + angle_offset;

        let pitch_sin = face_pitch.sin();
        let pitch_cos = face_pitch.cos();

        // Aim direction vector using yaw + pitch (unit length)
        let dir_x = shot_yaw.sin() * pitch_cos;
        let dir_y = pitch_sin;
        let dir_z = shot_yaw.cos() * pitch_cos;

        // Camera origin at eye height (match FPV) and push forward along aim direction
        let camera_origin = Vec3::new(
            shooter_pos.x,
            PLAYER_HEIGHT.mul_add(PLAYER_EYE_HEIGHT_RATIO, shooter_pos.y),
            shooter_pos.z,
        );
        let spawn_pos = camera_origin + Vec3::new(dir_x, dir_y, dir_z) * PROJECTILE_SPAWN_OFFSET;

        // Check if the path from player to spawn position crosses through a wall
        let spawn_position: Position = spawn_pos.into();
        let camera_pos: Position = camera_origin.into();

        // Check blocking conditions with short-circuit evaluation
        let is_blocked = is_blocked_by_wall(&camera_pos, &spawn_position, walls)
            || is_blocked_by_ramp(&spawn_position, ramps)
            || is_blocked_by_floor(&camera_pos, &spawn_position, floors);

        if is_blocked {
            continue;
        }

        spawns.push(ProjectileSpawnInfo {
            position: spawn_position,
            direction_yaw: shot_yaw,
            direction_pitch: face_pitch,
        });
    }

    spawns
}

fn is_blocked_by_wall(camera_pos: &Position, spawn_position: &Position, walls: &[Wall]) -> bool {
    walls
        .iter()
        .any(|wall| sweep_projectile_spawn_vs_wall(camera_pos, spawn_position, wall))
}

fn sweep_projectile_spawn_vs_wall(start: &Position, end: &Position, wall: &Wall) -> bool {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let is_horizontal = dx > dz;
    let wall_half_thickness = wall.width / 2.0;

    let center = Vec3::new(
        f32::midpoint(wall.x1, wall.x2),
        f32::from(wall.level).mul_add(LEVEL_HEIGHT, WALL_HEIGHT / 2.0),
        f32::midpoint(wall.z1, wall.z2),
    );
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { wall_half_thickness } + PROJECTILE_RADIUS,
        WALL_HEIGHT / 2.0 + PROJECTILE_RADIUS,
        if is_horizontal { wall_half_thickness } else { dz / 2.0 } + PROJECTILE_RADIUS,
    );

    sweep_point_vs_cuboid(start, Vec3::from(*end) - Vec3::from(*start), center, half_extents).is_some()
}

fn is_blocked_by_ramp(spawn_position: &Position, ramps: &[Ramp]) -> bool {
    // If the muzzle point sits inside the ramp volume (e.g., standing at the base facing the ramp), block the shot.
    ramps.iter().any(|ramp| {
        let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

        if spawn_position.x < min_x || spawn_position.x > max_x || spawn_position.z < min_z || spawn_position.z > max_z
        {
            return false;
        }

        let ramp_height = height_on_ramp(&[*ramp], spawn_position.x, spawn_position.z);
        ramp_height > 0.0 && spawn_position.y - PROJECTILE_RADIUS <= ramp_height
    })
}

fn is_blocked_by_floor(camera_pos: &Position, spawn_position: &Position, floors: &[Floor]) -> bool {
    floors
        .iter()
        .any(|floor| sweep_player_vs_floor(camera_pos, spawn_position, floor, PROJECTILE_RADIUS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_wall(level: u8) -> Wall {
        Wall {
            x1: -2.0,
            z1: 1.0,
            x2: 2.0,
            z2: 1.0,
            width: WALL_THICKNESS,
            level,
        }
    }

    #[test]
    fn spawn_path_ignores_wall_on_different_level() {
        let start = Position {
            x: 0.0,
            y: PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO,
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO,
            z: 2.0,
        };

        assert!(is_blocked_by_wall(&start, &end, &[test_wall(0)]));
        assert!(!is_blocked_by_wall(&start, &end, &[test_wall(1)]));
    }

    #[test]
    fn spawn_path_blocks_wall_on_same_upper_level() {
        let y = LEVEL_HEIGHT + PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO;
        let start = Position { x: 0.0, y, z: 0.0 };
        let end = Position { x: 0.0, y, z: 2.0 };

        assert!(is_blocked_by_wall(&start, &end, &[test_wall(1)]));
        assert!(!is_blocked_by_wall(&start, &end, &[test_wall(0)]));
    }
}
