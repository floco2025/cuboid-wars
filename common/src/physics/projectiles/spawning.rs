use crate::{
    config::GameplayConfig,
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
#[expect(
    clippy::too_many_arguments,
    reason = "spawn geometry reads shooter state, config, and world"
)]
pub fn calculate_projectile_spawns(
    shooter_pos: &Position,
    face_yaw: f32,
    face_pitch: f32,
    has_multi_shot: bool,
    shooter_eye_height: f32,
    gameplay: &GameplayConfig,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
) -> Vec<ProjectileSpawnInfo> {
    let mut spawns = Vec::new();

    let offsets: &[(f32, f32)] = if has_multi_shot {
        gameplay.projectiles.multi_shot.shots()
    } else {
        &[(0.0, 0.0)]
    };

    for &(yaw_offset, pitch_offset) in offsets {
        let shot_yaw = face_yaw + yaw_offset;
        let shot_pitch = face_pitch + pitch_offset;

        let aim = crate::math::direction_from_yaw_pitch(shot_yaw, shot_pitch);

        // Camera origin at eye height (match FPV) and push forward along aim direction
        let camera_origin = Vec3::new(shooter_pos.x, shooter_pos.y + shooter_eye_height, shooter_pos.z);
        let spawn_pos = camera_origin + aim * gameplay.projectiles.spawn_offset;

        let spawn_position: Position = spawn_pos.into();
        let camera_pos: Position = camera_origin.into();

        if projectile_spawn_is_blocked(
            &camera_pos,
            &spawn_position,
            gameplay.projectiles.radius,
            collision_world,
            open_kinds,
        ) {
            continue;
        }

        spawns.push(ProjectileSpawnInfo {
            position: spawn_position,
            direction_yaw: shot_yaw,
            direction_pitch: shot_pitch,
        });
    }

    spawns
}

pub(super) fn projectile_spawn_is_blocked(
    start: &Position,
    end: &Position,
    radius: f32,
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
    collision_world.projectile_spawn_overlaps_blocker(start_vec, radius, open_kinds)
        || collision_world
            .cast_moving_ball(start_vec, translation, radius)
            .is_some()
        || collision_world
            .cast_moving_ball_against_barriers(start_vec, translation, radius, open_kinds)
            .is_some()
}
