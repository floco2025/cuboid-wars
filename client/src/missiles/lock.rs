use bevy::math::Vec3;

use common::{
    config::CharacterPhysicsConfig,
    physics::CollisionWorld,
    protocol::{HomingTarget, Position},
};

use crate::characters::ball_character_hit;

// Thin ray for the world-occlusion test, so walls block locks at their real
// silhouette regardless of how generous the aim assist is.
const LOCK_RAY_RADIUS: f32 = 0.05;

// Pick the lock-on target under the aim ray: nearest candidate the ray
// passes within `assist_radius` of, capped at `max_distance`; world geometry
// and powered bridges occlude (barriers don't block sight, matching
// `line_of_sight_clear` semantics — the missile flies around them).
#[must_use]
pub fn acquire_lock(
    collision_world: &CollisionWorld,
    origin: Vec3,
    aim_dir: Vec3,
    max_distance: f32,
    assist_radius: f32,
    candidates: impl Iterator<Item = (HomingTarget, Position, f32, CharacterPhysicsConfig)>,
) -> Option<HomingTarget> {
    let translation = aim_dir.normalize_or_zero() * max_distance;
    if translation.length_squared() <= f32::EPSILON {
        return None;
    }
    let world_t = collision_world
        .cast_moving_ball(origin, translation, LOCK_RAY_RADIUS)
        .map_or(1.0, |hit| hit.t);
    let origin_pos = Position::from(origin);

    let mut best: Option<(f32, HomingTarget)> = None;
    for (target, pos, face_yaw, physics) in candidates {
        let Some(hit) = ball_character_hit(&origin_pos, translation, assist_radius, 1.0, &pos, face_yaw, physics)
        else {
            continue;
        };
        if hit.time_of_impact >= world_t {
            continue;
        }
        if best.is_none_or(|(best_t, _)| hit.time_of_impact < best_t) {
            best = Some((hit.time_of_impact, target));
        }
    }
    best.map(|(_, target)| target)
}

#[cfg(test)]
#[path = "tests/lock.rs"]
mod tests;
