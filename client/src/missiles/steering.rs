use bevy::prelude::*;
use std::f32::consts::TAU;

use common::{physics::CollisionWorld, protocol::BarrierKindId};

// How far inside the fuse boundary a terminal approach stops, so rounding
// cannot leave the route just outside it.
const MISSILE_APPROACH_FUSE_FRACTION: f32 = 0.9;

// Candidate fan around the blocked to-target direction, evaluated in order
// of deviation from it. No up/down preference: the clear test rejects
// directions into floors and walls, so a missile whose target is below
// naturally dives for an opening and one whose target is above climbs.
const AVOID_PITCH_DEGREES: [f32; 7] = [0.0, 35.0, -35.0, 70.0, -70.0, 85.0, -85.0];
const AVOID_YAW_DEGREES: [f32; 8] = [0.0, 45.0, -45.0, 90.0, -90.0, 135.0, -135.0, 180.0];
// Weave: two incommensurate frequencies (Hz) so the corkscrew never
// repeats cleanly; the wobble straightens out inside the fade distance so
// terminal accuracy is unaffected.
const WEAVE_HZ_A: f32 = 2.3;
const WEAVE_HZ_B: f32 = 3.1;
const WEAVE_FADE_DISTANCE: f32 = 6.0;
// Lead-pursuit cap: don't predict the target further ahead than this.
const MISSILE_LEAD_MAX_SECS: f32 = 1.0;
// A per-tick displacement faster than this is a teleport (respawn), not
// motion — leading it would aim into nowhere.
const MISSILE_LEAD_MAX_TARGET_SPEED: f32 = 15.0;

pub(super) fn sweep_clear(
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    translation: Vec3,
    radius: f32,
) -> bool {
    collision_world.projectile_path_clear(origin, translation, radius, open_kinds)
}

// A missile skimming a floor or a door frame is already within its radius of
// geometry; the fuse and the terminal approach judge the travel alone so it
// can still reach its target there.
pub(super) fn travel_clear(
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    translation: Vec3,
    radius: f32,
) -> bool {
    collision_world.projectile_sweep_clear(origin, translation, radius, open_kinds)
}

pub(super) fn terminal_approach(
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    target: Vec3,
    radius: f32,
    fuse_distance: f32,
) -> Option<Vec3> {
    let displacement = target - origin;
    if travel_clear(world, open_kinds, origin, displacement, radius) {
        return Some(target);
    }
    let travel = (displacement.length() - fuse_distance * MISSILE_APPROACH_FUSE_FRACTION).max(0.0);
    let approach = origin + displacement.normalize_or_zero() * travel;
    (world.attack_path_clear(approach, target, open_kinds)
        && travel_clear(world, open_kinds, origin, approach - origin, radius))
    .then_some(approach)
}

// Clear direction from the pitch × yaw fan closest to `desired`.
// `None` when every candidate is blocked (fully boxed in). `desired` itself
// is candidate zero: a blocked sight line to the target doesn't imply the
// lookahead-length sweep along it is blocked (the obstacle may sit beyond
// the lookahead).
pub(super) fn pick_clear_direction(
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    desired: Vec3,
    lookahead_distance: f32,
    radius: f32,
) -> Option<Vec3> {
    direction_candidates(desired).into_iter().find(|candidate| {
        sweep_clear(
            collision_world,
            open_kinds,
            origin,
            *candidate * lookahead_distance,
            radius,
        )
    })
}

fn direction_candidates(desired: Vec3) -> Vec<Vec3> {
    if desired == Vec3::ZERO {
        return Vec::new();
    }
    // Aiming near-vertical leaves no unique "toward up" plane; any
    // perpendicular works.
    let pitch_cross = desired.cross(Vec3::Y);
    let pitch_axis = if pitch_cross.length_squared() <= f32::EPSILON {
        desired.any_orthonormal_vector()
    } else {
        pitch_cross.normalize()
    };
    let mut candidates = Vec::with_capacity(AVOID_PITCH_DEGREES.len() * AVOID_YAW_DEGREES.len());
    for pitch_deg in AVOID_PITCH_DEGREES {
        // Rotating around `desired × up` by a positive angle tilts `desired`
        // toward +Y (right-hand rule); negative entries probe downward.
        let pitched = Quat::from_axis_angle(pitch_axis, pitch_deg.to_radians()) * desired;
        for yaw_deg in AVOID_YAW_DEGREES {
            let candidate = (Quat::from_rotation_y(yaw_deg.to_radians()) * pitched).normalize_or_zero();
            if candidate != Vec3::ZERO {
                candidates.push((desired.angle_between(candidate), candidate));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.into_iter().map(|(_, candidate)| candidate).collect()
}

pub(super) fn steer_clear(
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    velocity: Vec3,
    objective: Vec3,
    turn_radius: f32,
    delta: f32,
    lookahead_secs: f32,
    radius: f32,
) -> Vec3 {
    let desired = objective.normalize_or_zero();
    if delta <= 0.0 || desired == Vec3::ZERO || velocity.length_squared() <= f32::EPSILON {
        return velocity;
    }
    let lookahead_secs = lookahead_secs.max(delta);
    let clear_time = |direction| {
        turn_clear_time(
            world,
            open_kinds,
            origin,
            velocity,
            direction,
            turn_radius,
            delta,
            lookahead_secs,
            radius,
        )
    };
    if clear_time(desired) >= lookahead_secs {
        return steer(velocity, desired, turn_radius, delta);
    }
    let mut best = desired;
    let mut best_time = -1.0;
    for candidate in std::iter::once(velocity.normalize()).chain(direction_candidates(desired)) {
        let time = clear_time(candidate);
        if time > best_time {
            best = candidate;
            best_time = time;
        }
        if time >= lookahead_secs {
            break;
        }
    }
    steer(velocity, best, turn_radius, delta)
}

fn turn_clear_time(
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    mut origin: Vec3,
    mut velocity: Vec3,
    desired: Vec3,
    turn_radius: f32,
    delta: f32,
    lookahead_secs: f32,
    radius: f32,
) -> f32 {
    let mut elapsed = 0.0;
    while elapsed < lookahead_secs {
        let step = delta.min(lookahead_secs - elapsed);
        velocity = steer(velocity, desired, turn_radius, step);
        let translation = velocity * step;
        if !sweep_clear(world, open_kinds, origin, translation, radius) {
            break;
        }
        origin += translation;
        elapsed += step;
    }
    elapsed
}

// Bend the homing direction with a decaying corkscrew wobble — cosmetic
// flight character. The perturbation is a fixed fraction of the direction
// (constant angular amplitude) and fades to zero over the last
// `WEAVE_FADE_DISTANCE` meters.
pub(super) fn weave_direction(to_target: Vec3, elapsed: f32, phase: f32, strength: f32) -> Vec3 {
    let distance = to_target.length();
    if strength <= 0.0 || distance <= f32::EPSILON {
        return to_target;
    }
    let dir = to_target / distance;
    let side = dir.any_orthonormal_vector();
    let up_ish = dir.cross(side);
    let fade = (distance / WEAVE_FADE_DISTANCE).clamp(0.0, 1.0);
    let swing_a = (elapsed * WEAVE_HZ_A * TAU + phase).sin();
    let swing_b = (elapsed * WEAVE_HZ_B * TAU + phase * 1.7).cos();
    let wobble = (side * swing_a + up_ish * swing_b) * strength * fade;
    (dir + wobble).normalize_or(dir) * distance
}

pub(super) fn closest_point_on_segment(start: Vec3, travel: Vec3, point: Vec3) -> Vec3 {
    let length_squared = travel.length_squared();
    if length_squared <= f32::EPSILON {
        return start;
    }
    let t = ((point - start).dot(travel) / length_squared).clamp(0.0, 1.0);
    start + travel * t
}

pub(super) fn target_velocity_estimate(last_center: Option<Vec3>, center: Vec3, delta: f32) -> Vec3 {
    if delta <= f32::EPSILON {
        return Vec3::ZERO;
    }
    let Some(last_center) = last_center else {
        return Vec3::ZERO;
    };
    let velocity = (center - last_center) / delta;
    if velocity.length_squared() > MISSILE_LEAD_MAX_TARGET_SPEED * MISSILE_LEAD_MAX_TARGET_SPEED {
        Vec3::ZERO
    } else {
        velocity
    }
}

pub(super) fn lead_point(origin: Vec3, target_center: Vec3, target_velocity: Vec3, missile_speed: f32) -> Vec3 {
    if missile_speed <= f32::EPSILON {
        return target_center;
    }
    let lead_time = (origin.distance(target_center) / missile_speed).min(MISSILE_LEAD_MAX_SECS);
    target_center + target_velocity * lead_time
}

// Rotate the velocity direction toward the objective along a circle of
// `turn_radius` at the current speed (at most `speed / turn_radius * delta`
// radians), preserving speed.
pub(super) fn steer(velocity: Vec3, to_objective: Vec3, turn_radius: f32, delta: f32) -> Vec3 {
    let speed = velocity.length();
    if speed <= f32::EPSILON {
        return velocity;
    }
    let current = velocity / speed;
    let desired = to_objective.normalize_or_zero();
    if desired == Vec3::ZERO {
        return velocity;
    }
    let angle = current.angle_between(desired);
    let max_step = speed / turn_radius * delta;
    if angle <= max_step {
        return desired * speed;
    }
    let cross = current.cross(desired);
    // Anti-parallel objective: no unique rotation plane, pick any.
    let axis = if cross.length_squared() <= f32::EPSILON {
        current.any_orthonormal_vector()
    } else {
        cross.normalize()
    };
    (Quat::from_axis_angle(axis, max_step) * current) * speed
}

#[cfg(test)]
#[path = "tests/steering.rs"]
mod tests;
