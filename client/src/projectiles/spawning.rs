use bevy::math::Vec3;
use common::{
    config::GameplayConfig,
    math::direction_from_yaw_pitch,
    physics::CollisionWorld,
    protocol::{BarrierKindId, Position},
};

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

// Whether the muzzle check runs. The shooter drops a shot its own muzzle
// blocks; an observer reproduces the shooter's set instead, because carriers
// and plate state have moved on by the time the volley is relayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuzzleCheck {
    Enforced,
    Skipped,
}

// Calculate valid projectile spawn positions for a shot
//
// Returns a list of projectiles that should be spawned, excluding any that would
// be blocked by walls on the way from the muzzle to the spawn point.
#[must_use]
pub fn calculate_projectile_spawns(
    origin: &Position,
    face_yaw: f32,
    face_pitch: f32,
    pattern: u8,
    gameplay: &GameplayConfig,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    muzzle_check: MuzzleCheck,
) -> Vec<ProjectileSpawnInfo> {
    let mut spawns = Vec::new();

    let Some(offsets) = gameplay.projectiles.multi_shot.shot_offsets(pattern) else {
        return spawns;
    };

    for &(yaw_offset, pitch_offset) in offsets {
        let shot_yaw = face_yaw + yaw_offset;
        let shot_pitch = face_pitch + pitch_offset;

        let aim = direction_from_yaw_pitch(shot_yaw, shot_pitch);

        let camera_origin = Vec3::from(*origin);
        let spawn_pos = camera_origin + aim * gameplay.projectiles.spawn_offset;

        let spawn_position: Position = spawn_pos.into();
        let camera_pos: Position = camera_origin.into();

        if muzzle_check == MuzzleCheck::Enforced
            && projectile_spawn_is_blocked(
                &camera_pos,
                &spawn_position,
                gameplay.projectiles.radius,
                collision_world,
                open_kinds,
            )
        {
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

    // Surfaces and barriers live in separate filter groups; check both
    // along the muzzle→spawn segment. Without the barrier cast, a shooter
    // pressed against a barrier could spawn the projectile on the far side
    // of it. Open (plate-held) kinds are excluded from the barrier checks —
    // they're gone visually, so shots pass cleanly through them.
    collision_world.projectile_spawn_overlaps_blocker(start_vec, radius, open_kinds)
        || collision_world
            .cast_moving_ball(start_vec, translation, radius)
            .is_some()
        || collision_world
            .cast_moving_ball_against_fields(start_vec, translation, radius, open_kinds)
            .is_some()
}
