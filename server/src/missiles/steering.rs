use bevy::prelude::*;
use std::f32::consts::TAU;

use common::{physics::CollisionWorld, protocol::BarrierKindId};

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
    collision_world.cast_moving_ball(origin, translation, radius).is_none()
        && collision_world
            .cast_moving_ball_against_barriers(origin, translation, radius, open_kinds)
            .is_none()
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
    lookahead: f32,
    radius: f32,
) -> Option<Vec3> {
    if desired == Vec3::ZERO {
        return None;
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
    candidates
        .into_iter()
        .find(|(_, candidate)| sweep_clear(collision_world, open_kinds, origin, *candidate * lookahead, radius))
        .map(|(_, candidate)| candidate)
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
mod tests {
    use super::*;
    use common::{
        constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS},
        protocol::{BarrierKindTable, Floor, MapLayout, Wall},
    };
    use std::f32::consts::SQRT_2;

    #[test]
    fn steer_clamps_rotation_to_the_turn_radius_arc() {
        let velocity = Vec3::Z * 12.0;
        let steered = steer(velocity, Vec3::X, 6.0, 0.1);
        let angle = velocity.angle_between(steered);
        assert!((angle - 0.2).abs() < 1e-4, "expected a 0.2 rad step, got {angle}");
    }

    #[test]
    fn steer_preserves_speed() {
        let velocity = Vec3::new(3.0, 4.0, 12.0);
        let steered = steer(velocity, Vec3::new(-5.0, 0.2, 1.0), 3.0, 0.033);
        assert!((steered.length() - velocity.length()).abs() < 1e-3);
    }

    #[test]
    fn steer_aligns_when_within_one_step() {
        let velocity = Vec3::Z * 10.0;
        let objective = Vec3::new(0.05, 0.0, 1.0);
        let steered = steer(velocity, objective, 7.0, 0.1);
        assert!(steered.normalize().dot(objective.normalize()) > 0.9999);
    }

    #[test]
    fn steer_without_objective_keeps_velocity() {
        let velocity = Vec3::Z * 10.0;
        assert_eq!(steer(velocity, Vec3::ZERO, 7.0, 0.1), velocity);
    }

    #[test]
    fn closest_point_on_segment_finds_the_nearest_pass() {
        let start = Vec3::new(0.0, 0.0, -2.0);
        let travel = Vec3::new(0.0, 0.0, 4.0);
        // Target abeam of the segment's midpoint: closest pass is at z=1.
        let target = Vec3::new(1.0, 0.0, 1.0);

        let closest = closest_point_on_segment(start, travel, target);

        assert!((closest - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-5);
        // Past the segment end the clamp holds.
        let beyond = closest_point_on_segment(start, travel, Vec3::new(0.0, 0.0, 10.0));
        assert_eq!(beyond, Vec3::new(0.0, 0.0, 2.0));
    }

    #[test]
    fn weave_zero_strength_is_straight() {
        let to_target = Vec3::new(3.0, 1.0, 20.0);
        assert_eq!(weave_direction(to_target, 1.234, 0.7, 0.0), to_target);
    }

    #[test]
    fn weave_bends_within_bounds_and_fades_when_close() {
        let far = Vec3::Z * 30.0;
        let bent = weave_direction(far, 0.4, 1.0, 0.35);
        let angle = far.angle_between(bent);
        assert!(angle > 0.0, "far from the target the path wobbles");
        // Max deviation: |wobble| <= strength * sqrt(2).
        assert!(angle <= (0.35_f32 * SQRT_2).atan() + 1e-3);
        assert!((bent.length() - far.length()).abs() < 1e-3, "range is preserved");

        let near = Vec3::Z * 0.5;
        let near_bent = weave_direction(near, 0.4, 1.0, 0.35);
        assert!(
            near.angle_between(near_bent) < 0.35 * (0.5 / WEAVE_FADE_DISTANCE) * SQRT_2 + 1e-3,
            "the wobble fades on final approach"
        );
    }

    #[test]
    fn lead_point_aims_ahead_of_a_moving_target() {
        let origin = Vec3::ZERO;
        let target = Vec3::new(0.0, 0.0, 12.0);
        let velocity = Vec3::new(4.0, 0.0, 0.0);

        let point = lead_point(origin, target, velocity, 12.0);

        // 12 m away at 12 m/s → 1 s of lead → 4 m ahead along the retreat.
        assert!((point - Vec3::new(4.0, 0.0, 12.0)).length() < 1e-4);
    }

    #[test]
    fn lead_point_static_target_is_the_target() {
        let target = Vec3::new(3.0, 1.0, 7.0);
        assert_eq!(lead_point(Vec3::ZERO, target, Vec3::ZERO, 12.0), target);
    }

    #[test]
    fn target_velocity_estimate_ignores_teleports() {
        let last = Some(Vec3::ZERO);
        let walked = target_velocity_estimate(last, Vec3::new(0.0, 0.0, 0.132), 0.033);
        assert!((walked.z - 4.0).abs() < 0.01, "normal motion is estimated");

        let jumped = target_velocity_estimate(last, Vec3::new(40.0, 0.0, 0.0), 0.033);
        assert_eq!(jumped, Vec3::ZERO, "a respawn jump reads as stationary");
    }

    #[test]
    fn pick_clear_direction_prefers_climbing_over_a_wall() {
        // A wide wall dead ahead: the 35° climb still clips it within the
        // lookahead, the 70° climb passes above — the pick must go up, not
        // sideways.
        let layout = MapLayout {
            walls: vec![Wall {
                x1: -20.0,
                z1: 2.0,
                x2: 20.0,
                z2: 2.0,
                width: WALL_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let origin = Vec3::new(0.0, 2.0, 0.0);

        let picked =
            pick_clear_direction(&world, &[], origin, Vec3::Z, 7.2, 0.3).expect("an upward candidate should be clear");

        assert!(picked.y > 0.5, "expected a climbing direction, got {picked}");
    }

    #[test]
    fn pick_clear_direction_in_open_space_returns_desired() {
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let picked = pick_clear_direction(&world, &[], Vec3::new(0.0, 5.0, 0.0), Vec3::Z, 7.2, 0.3)
            .expect("open space always has a clear candidate");
        assert!(
            picked.angle_between(Vec3::Z).to_degrees() < 1.0,
            "nothing blocked: fly at the target"
        );
    }

    #[test]
    fn pick_clear_direction_dives_toward_a_target_below() {
        // A floor slab between the missile (above) and its target (below),
        // open past z = 2: the pick must descend toward the opening, not
        // cruise on the upper level.
        let layout = MapLayout {
            floors: vec![Floor {
                x1: -10.0,
                z1: -2.0,
                x2: 10.0,
                z2: 2.0,
                y: LEVEL_HEIGHT,
                thickness: FLOOR_THICKNESS,
                level: 1,
            }],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        // Straight down onto the slab is blocked; the fan must find the
        // descending direction past the slab edge.
        let picked = pick_clear_direction(&world, &[], Vec3::new(0.0, 6.0, 0.0), Vec3::NEG_Y, 7.2, 0.3)
            .expect("a descending candidate past the slab edge should be clear");

        assert!(picked.y < -0.2, "expected a diving direction, got {picked}");
    }
}
