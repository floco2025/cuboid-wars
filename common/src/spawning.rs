use crate::{
    constants::*,
    map::ramp_surface_at,
    physics::{floor_cuboid, segment_intersects_cuboid, wall_cuboid},
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
        POWER_UP_MULTI_SHOT_MULTIPLIER
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
            || is_blocked_by_ramp(&camera_pos, &spawn_position, ramps)
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
    let cuboid = wall_cuboid(wall, PROJECTILE_RADIUS);
    segment_intersects_cuboid(start, end, cuboid)
}

fn is_blocked_by_ramp(camera_pos: &Position, spawn_position: &Position, ramps: &[Ramp]) -> bool {
    ramps.iter().any(|ramp| {
        let start_depth = projectile_depth_inside_ramp(camera_pos, ramp);
        let end_depth = projectile_depth_inside_ramp(spawn_position, ramp);

        match (start_depth, end_depth) {
            (None, None) | (Some(_), None) => false,
            (None, Some(_)) => true,
            (Some(start), Some(end)) => end >= start - PHYSICS_EPSILON,
        }
    })
}

fn projectile_depth_inside_ramp(pos: &Position, ramp: &Ramp) -> Option<f32> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
        return None;
    }

    let (min_y, _) = ramp.bounds_y();
    if pos.y + PROJECTILE_RADIUS < min_y {
        return None;
    }

    let ramp_height = ramp_surface_at(ramp, pos.x, pos.z);
    if ramp_height <= min_y + PHYSICS_EPSILON {
        return None;
    }

    let top_depth = ramp_height - (pos.y - PROJECTILE_RADIUS);
    if top_depth < 0.0 {
        return None;
    }

    let side_depth = (pos.x - min_x).min(max_x - pos.x).min(pos.z - min_z).min(max_z - pos.z);
    Some(side_depth.min(top_depth))
}

fn is_blocked_by_floor(camera_pos: &Position, spawn_position: &Position, floors: &[Floor]) -> bool {
    floors
        .iter()
        .any(|floor| sweep_projectile_spawn_vs_floor(camera_pos, spawn_position, floor))
}

fn sweep_projectile_spawn_vs_floor(start: &Position, end: &Position, floor: &Floor) -> bool {
    let cuboid = floor_cuboid(floor, PROJECTILE_RADIUS);
    segment_intersects_cuboid(start, end, cuboid)
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

    fn test_floor(level: u8) -> Floor {
        let y = f32::from(level) * LEVEL_HEIGHT;
        Floor {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y,
            thickness: FLOOR_THICKNESS,
            level,
        }
    }

    fn test_ramp() -> Ramp {
        Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 8.0,
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

    #[test]
    fn spawn_path_blocks_when_starting_inside_wall() {
        let start = Position {
            x: 0.0,
            y: PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO,
            z: 1.0,
        };
        let end = Position {
            x: 0.0,
            y: PLAYER_HEIGHT * PLAYER_EYE_HEIGHT_RATIO,
            z: 2.0,
        };

        assert!(is_blocked_by_wall(&start, &end, &[test_wall(0)]));
    }

    #[test]
    fn spawn_path_floor_check_catches_crossing_segment() {
        let floor = test_floor(1);
        let start = Position {
            x: 0.0,
            y: LEVEL_HEIGHT + 1.0,
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: LEVEL_HEIGHT - 1.0,
            z: 0.0,
        };

        assert!(is_blocked_by_floor(&start, &end, &[floor]));
    }

    #[test]
    fn spawn_path_floor_check_blocks_start_inside() {
        let floor = test_floor(1);
        let start = Position {
            x: 0.0,
            y: LEVEL_HEIGHT,
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: LEVEL_HEIGHT + 1.0,
            z: 0.0,
        };

        assert!(is_blocked_by_floor(&start, &end, &[floor]));
    }

    #[test]
    fn spawn_path_allows_ramp_side_escape() {
        let ramp = test_ramp();
        let start = Position { x: 0.2, y: 1.4, z: 4.0 };
        let end = Position {
            x: 0.05,
            y: 1.4,
            z: 4.0,
        };

        assert!(!is_blocked_by_ramp(&start, &end, &[ramp]));
    }

    #[test]
    fn spawn_path_blocks_into_ramp_side() {
        let ramp = test_ramp();
        let start = Position { x: 0.2, y: 1.4, z: 4.0 };
        let end = Position { x: 0.8, y: 1.4, z: 4.0 };

        assert!(is_blocked_by_ramp(&start, &end, &[ramp]));
    }

    #[test]
    fn spawn_path_blocks_entering_ramp_from_outside() {
        let ramp = test_ramp();
        let start = Position {
            x: -0.2,
            y: 1.4,
            z: 4.0,
        };
        let end = Position { x: 0.2, y: 1.4, z: 4.0 };

        assert!(is_blocked_by_ramp(&start, &end, &[ramp]));
    }
}
