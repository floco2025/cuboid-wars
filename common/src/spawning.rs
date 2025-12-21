use crate::{
    collision::{sweep_player_vs_roof, sweep_player_vs_wall},
    constants::*,
    map::height_on_ramp,
    protocol::{Position, Ramp, Roof, Wall},
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
    roofs: &[Roof],
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
        let spawn_position = Position {
            x: spawn_pos.x,
            y: spawn_pos.y,
            z: spawn_pos.z,
        };

        // If the spawn height sits above the top of ground walls, skip wall blocking (roof-edge shots, ramps)
        let spawn_above_walls = spawn_position.y - PROJECTILE_RADIUS >= WALL_HEIGHT;
        let blocked_by_wall = !spawn_above_walls
            && walls.iter().any(|wall| {
                sweep_player_vs_wall(
                    &Position {
                        x: camera_origin.x,
                        y: camera_origin.y,
                        z: camera_origin.z,
                    },
                    &spawn_position,
                    wall,
                )
            });

        // If the muzzle point sits inside the ramp volume (e.g., standing at the base facing the ramp), block the shot.
        let blocked_by_ramp = ramps.iter().any(|ramp| {
            let min_x = ramp.x1.min(ramp.x2);
            let max_x = ramp.x1.max(ramp.x2);
            let min_z = ramp.z1.min(ramp.z2);
            let max_z = ramp.z1.max(ramp.z2);

            if spawn_position.x < min_x
                || spawn_position.x > max_x
                || spawn_position.z < min_z
                || spawn_position.z > max_z
            {
                return false;
            }

            let ramp_height = height_on_ramp(&[*ramp], spawn_position.x, spawn_position.z);
            ramp_height > 0.0 && spawn_position.y - PROJECTILE_RADIUS <= ramp_height
        });

        let is_spawn_blocked = blocked_by_wall || blocked_by_ramp;

        // Block shots whose muzzle-to-spawn segment intersects the roof slab volume (sweep-style test)
        let blocked_by_roof = roofs.iter().any(|roof| {
            sweep_player_vs_roof(
                &Position {
                    x: camera_origin.x,
                    y: camera_origin.y,
                    z: camera_origin.z,
                },
                &spawn_position,
                roof,
                PROJECTILE_RADIUS,
            )
        });

        let is_spawn_blocked = is_spawn_blocked || blocked_by_roof;
        // Skip this projectile if the spawn path is blocked
        if is_spawn_blocked {
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
