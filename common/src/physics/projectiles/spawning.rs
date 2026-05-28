use crate::{
    constants::*,
    physics::CollisionWorld,
    protocol::{BarrierKindId, Position},
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
    shooter_eye_height: f32,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
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
        let camera_origin = Vec3::new(shooter_pos.x, shooter_pos.y + shooter_eye_height, shooter_pos.z);
        let spawn_pos = camera_origin + Vec3::new(dir_x, dir_y, dir_z) * PROJECTILE_SPAWN_OFFSET;

        // Check if the path from player to spawn position crosses through a wall
        let spawn_position: Position = spawn_pos.into();
        let camera_pos: Position = camera_origin.into();

        if projectile_spawn_is_blocked(&camera_pos, &spawn_position, collision_world, open_kinds) {
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

fn projectile_spawn_is_blocked(
    start: &Position,
    end: &Position,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
) -> bool {
    let start_vec = Vec3::from(*start);
    let end_vec = Vec3::from(*end);
    let translation = end_vec - start_vec;

    // Walls/floors/ramps and barriers live in separate filter groups; check
    // both along the muzzle→spawn segment. Without the barrier cast, a
    // shooter pressed against a barrier could spawn the projectile on the
    // far side of it. Open (plate-held) kinds are excluded from the barrier
    // checks — they're gone visually, so shots pass cleanly through them.
    collision_world.projectile_spawn_overlaps_blocker(start_vec, PROJECTILE_RADIUS, open_kinds)
        || collision_world
            .cast_moving_ball(start_vec, translation, PROJECTILE_RADIUS)
            .is_some()
        || collision_world
            .cast_moving_ball_against_barriers(start_vec, translation, PROJECTILE_RADIUS, open_kinds)
            .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GameplayConfig;
    use crate::protocol::{Floor, MapLayout, Ramp, Wall};

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

    fn collision_world(walls: &[Wall], ramps: &[Ramp], floors: &[Floor]) -> CollisionWorld {
        CollisionWorld::from_map_layout(
            &MapLayout {
                walls: walls.to_vec(),
                ramps: ramps.to_vec(),
                floors: floors.to_vec(),
                ..Default::default()
            },
            &crate::protocol::BarrierKindTable::default(),
        )
    }

    fn player_eye_height() -> f32 {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .player
            .eye_height()
    }

    #[test]
    fn spawn_path_ignores_wall_on_different_level() {
        let start = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 0.0,
        };
        let end = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 2.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[test_wall(1)], &[], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_wall_on_same_upper_level() {
        let y = LEVEL_HEIGHT + player_eye_height();
        let start = Position { x: 0.0, y, z: 0.0 };
        let end = Position { x: 0.0, y, z: 2.0 };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[test_wall(1)], &[], &[]),
            &[]
        ));
        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_when_starting_inside_wall() {
        let start = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 1.0,
        };
        let end = Position {
            x: 0.0,
            y: player_eye_height(),
            z: 2.0,
        };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[test_wall(0)], &[], &[]),
            &[]
        ));
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

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[], &[], &[floor]),
            &[]
        ));
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

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[], &[], &[floor]),
            &[]
        ));
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

        assert!(!projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
    }

    #[test]
    fn spawn_path_blocks_into_ramp_side() {
        let ramp = test_ramp();
        let start = Position { x: 0.2, y: 1.4, z: 4.0 };
        let end = Position { x: 0.8, y: 1.4, z: 4.0 };

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
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

        assert!(projectile_spawn_is_blocked(
            &start,
            &end,
            &collision_world(&[], &[ramp], &[]),
            &[]
        ));
    }
}
