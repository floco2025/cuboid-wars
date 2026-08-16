use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::ActorMap,
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    missiles::{AirGraph, MissileInfo, MissileMap, MissileVelocity},
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    constants::GRID_CELL_SIZE,
    physics::{CollisionWorld, character_center},
    protocol::{
        ActorMarker, BarrierKindId, FaceDirection, HomingTarget, MissileId, MissileMarker, PlayerMarker, Position,
    },
};

// Net-displacement watchdog distance, 3D (missiles fly): a missile that
// doesn't displace this far within `stall_secs` is orbiting or wedged and
// self-detonates.
const MISSILE_STALL_PROGRESS_DISTANCE: f32 = 1.0;
// How long a picked avoidance direction is flown before re-deciding, so the
// missile doesn't dither between candidates every tick.
const MISSILE_AVOID_COMMIT_SECS: f32 = 0.25;
// How far ahead (in seconds of travel) a candidate direction must be clear.
const MISSILE_AVOID_LOOKAHEAD_SECS: f32 = 0.6;
// Candidate fan around the blocked to-target direction, evaluated in order
// of deviation from it. No up/down preference: the clear test rejects
// directions into floors and walls, so a missile whose target is below
// naturally dives for an opening and one whose target is above climbs.
const AVOID_PITCH_DEGREES: [f32; 7] = [0.0, 35.0, -35.0, 70.0, -70.0, 85.0, -85.0];
const AVOID_YAW_DEGREES: [f32; 8] = [0.0, 45.0, -45.0, 90.0, -90.0, 135.0, -135.0, 180.0];
// Waypoint following along the air-graph route.
const MISSILE_WAYPOINT_REACH_DISTANCE: f32 = 1.7;
const MISSILE_PATH_RETRY_SECS: f32 = 0.5;
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

type MissileGuidanceQuery<'w, 's> =
    Query<'w, 's, (&'static MissileId, &'static Position, &'static mut MissileVelocity), With<MissileMarker>>;

type TargetPositionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceDirection),
    (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<MissileMarker>),
>;

#[derive(SystemParam)]
pub struct MissileGuidanceParams<'w, 's> {
    missiles: ResMut<'w, MissileMap>,
    missile_query: MissileGuidanceQuery<'w, 's>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
    target_data: TargetPositionQuery<'w, 's>,
    air_graph: Res<'w, AirGraph>,
    collision_world: Res<'w, CollisionWorld>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    gameplay_config: Res<'w, GameplayConfig>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
}

pub fn missiles_guidance_system(time: Res<Time>, mut params: MissileGuidanceParams) {
    let delta = time.delta_secs();
    let config = params.server_gameplay_config.missiles;

    for (id, pos, mut velocity) in &mut params.missile_query {
        let Some(info) = params.missiles.get_mut(id) else {
            continue;
        };

        info.lifetime_timer -= delta;
        if info.lifetime_timer <= 0.0 {
            info.detonate_at = Some(*pos);
            continue;
        }

        let resolved = info.target.and_then(|target| {
            resolve_target(
                target,
                &params.players,
                &params.actors,
                &params.target_data,
                &params.gameplay_config,
            )
        });
        if resolved.is_none() {
            // Dead/vanished target: fly straight on the last heading and let
            // contact or lifetime finish the flight.
            info.target = None;
        }

        if let Some((_, target_center)) = resolved {
            let origin = Vec3::from(*pos);

            // Lead pursuit: aim at the predicted intercept — pure pursuit
            // tail-chases a retreating target forever.
            let target_velocity = target_velocity_estimate(info.last_target_center, target_center, delta);
            info.last_target_center = Some(target_center);
            let aim_point = lead_point(origin, target_center, target_velocity, config.speed);

            let objective_dir = if missile_sight_clear(
                &params.collision_world,
                &params.open_barrier_kinds.0,
                pos,
                target_center,
                config.radius,
            ) {
                info.avoid_dir = None;
                let elapsed = config.lifetime_secs - info.lifetime_timer;
                weave_direction(aim_point - origin, elapsed, info.weave_phase, config.weave_strength)
            } else {
                // No line of sight: route through the 3D airspace graph.
                info.path_retry_timer -= delta;
                let target_moved = info
                    .path_target
                    .is_some_and(|prev| prev.distance_squared(target_center) > GRID_CELL_SIZE * GRID_CELL_SIZE);
                if info.path.is_empty() || target_moved || info.path_retry_timer <= 0.0 {
                    match params.air_graph.path(origin, target_center) {
                        Some(path) => {
                            info.path = path;
                            info.path_target = Some(target_center);
                        }
                        None => {
                            info.path.clear();
                            info.path_target = None;
                        }
                    }
                    info.path_retry_timer = MISSILE_PATH_RETRY_SECS;
                }
                advance_waypoints(
                    &mut info.path,
                    origin,
                    &params.collision_world,
                    &params.open_barrier_kinds.0,
                    config.radius,
                );
                if let Some(waypoint) = info.path.front() {
                    info.avoid_dir = None;
                    *waypoint - origin
                } else {
                    // No air route (sealed target, off-graph edge case):
                    // local dodge fan as a last resort.
                    info.avoid_timer -= delta;
                    let lookahead = config.speed * MISSILE_AVOID_LOOKAHEAD_SECS;
                    let desired = (aim_point - origin).normalize_or_zero();
                    let committed = info.avoid_dir.filter(|dir| {
                        info.avoid_timer > 0.0
                            && sweep_clear(
                                &params.collision_world,
                                &params.open_barrier_kinds.0,
                                origin,
                                *dir * lookahead,
                                config.radius,
                            )
                    });
                    let chosen = committed.or_else(|| {
                        let picked = pick_clear_direction(
                            &params.collision_world,
                            &params.open_barrier_kinds.0,
                            origin,
                            desired,
                            lookahead,
                            config.radius,
                        );
                        info.avoid_dir = picked;
                        info.avoid_timer = MISSILE_AVOID_COMMIT_SECS;
                        picked
                    });
                    // Nothing clear: press at the target anyway; the stall
                    // watchdog detonates a wedged missile.
                    chosen.unwrap_or(desired)
                }
            };
            velocity.0 = steer(velocity.0, objective_dir, config.turn_rate, delta);

            // Proximity fuse on this tick's travel SEGMENT, not a point
            // sample — at 0.4 m per tick a point check can alias straight
            // past the closest approach. Detonating at the closest point
            // keeps the target inside the full-damage blast core.
            let fuse = config.proximity_fuse_distance;
            let closest = closest_point_on_segment(origin, velocity.0 * delta, target_center);
            if closest.distance_squared(target_center) <= fuse * fuse {
                info.detonate_at = Some(closest.into());
                continue;
            }
        }

        if tick_missile_stall(info, pos, delta, config.stall_secs) {
            info.detonate_at = Some(*pos);
        }
    }
}

fn resolve_target(
    target: HomingTarget,
    players: &PlayerMap,
    actors: &ActorMap,
    target_data: &TargetPositionQuery,
    gameplay_config: &GameplayConfig,
) -> Option<(Position, Vec3)> {
    match target {
        HomingTarget::Player(id) => {
            let info = players.get(&id)?;
            if info.is_dead() {
                return None;
            }
            let (pos, _) = target_data.get(info.entity).ok()?;
            Some((*pos, character_center(*pos, gameplay_config.player.physics())))
        }
        HomingTarget::Actor(id) => {
            let info = actors.get(&id)?;
            let (pos, _) = target_data.get(info.entity).ok()?;
            Some((
                *pos,
                character_center(*pos, gameplay_config.validated_actor(&info.spawn_kind).physics()),
            ))
        }
    }
}

// Barrier-aware sight: `line_of_sight_clear` deliberately ignores barriers,
// but a missile beelining into a keyed barrier grill would just detonate on
// it — path around instead.
fn missile_sight_clear(
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    pos: &Position,
    target_center: Vec3,
    radius: f32,
) -> bool {
    let origin = Vec3::from(*pos);
    sweep_clear(collision_world, open_kinds, origin, target_center - origin, radius)
}

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
fn pick_clear_direction(
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
fn weave_direction(to_target: Vec3, elapsed: f32, phase: f32, strength: f32) -> Vec3 {
    let distance = to_target.length();
    if strength <= 0.0 || distance <= f32::EPSILON {
        return to_target;
    }
    let dir = to_target / distance;
    let side = dir.any_orthonormal_vector();
    let up_ish = dir.cross(side);
    let fade = (distance / WEAVE_FADE_DISTANCE).clamp(0.0, 1.0);
    let swing_a = (elapsed * WEAVE_HZ_A * std::f32::consts::TAU + phase).sin();
    let swing_b = (elapsed * WEAVE_HZ_B * std::f32::consts::TAU + phase * 1.7).cos();
    let wobble = (side * swing_a + up_ish * swing_b) * strength * fade;
    (dir + wobble).normalize_or(dir) * distance
}

// Pop reached waypoints, then string-pull: while the SECOND waypoint is
// already reachable in a straight sweep, drop the first — cell-granular BFS
// corners fly as smooth diagonals.
fn advance_waypoints(
    path: &mut std::collections::VecDeque<Vec3>,
    origin: Vec3,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    radius: f32,
) {
    while path.front().is_some_and(|wp| {
        wp.distance_squared(origin) <= MISSILE_WAYPOINT_REACH_DISTANCE * MISSILE_WAYPOINT_REACH_DISTANCE
    }) {
        path.pop_front();
    }
    while path.len() >= 2 {
        let Some(next) = path.get(1) else {
            break;
        };
        if sweep_clear(collision_world, open_kinds, origin, *next - origin, radius) {
            path.pop_front();
        } else {
            break;
        }
    }
}

fn closest_point_on_segment(start: Vec3, travel: Vec3, point: Vec3) -> Vec3 {
    let length_squared = travel.length_squared();
    if length_squared <= f32::EPSILON {
        return start;
    }
    let t = ((point - start).dot(travel) / length_squared).clamp(0.0, 1.0);
    start + travel * t
}

fn target_velocity_estimate(last_center: Option<Vec3>, center: Vec3, delta: f32) -> Vec3 {
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

fn lead_point(origin: Vec3, target_center: Vec3, target_velocity: Vec3, missile_speed: f32) -> Vec3 {
    if missile_speed <= f32::EPSILON {
        return target_center;
    }
    let lead_time = (origin.distance(target_center) / missile_speed).min(MISSILE_LEAD_MAX_SECS);
    target_center + target_velocity * lead_time
}

// Rotate the velocity direction toward the objective by at most
// `turn_rate * delta` radians, preserving speed.
fn steer(velocity: Vec3, to_objective: Vec3, turn_rate: f32, delta: f32) -> Vec3 {
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
    let max_step = turn_rate * delta;
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

// `tick_stall` for missiles: 3D displacement (missiles fly), detonate
// instead of escape.
fn tick_missile_stall(info: &mut MissileInfo, pos: &Position, delta: f32, window_secs: f32) -> bool {
    let Some(anchor) = info.stall_anchor else {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    };
    if anchor.distance_sq(pos) > MISSILE_STALL_PROGRESS_DISTANCE * MISSILE_STALL_PROGRESS_DISTANCE {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    }
    info.stall_timer -= delta;
    info.stall_timer <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{
        constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS},
        protocol::{BarrierKindTable, Floor, MapLayout, PlayerId, Wall},
    };
    use std::collections::VecDeque;

    fn missile_info() -> MissileInfo {
        MissileInfo {
            entity: Entity::PLACEHOLDER,
            shooter: PlayerId(1),
            target: None,
            path: VecDeque::new(),
            path_target: None,
            path_retry_timer: 0.0,
            avoid_dir: None,
            avoid_timer: 0.0,
            last_target_center: None,
            weave_phase: 0.0,
            lifetime_timer: 10.0,
            stall_anchor: None,
            stall_timer: 2.0,
            armed: false,
            last_broadcast_dir: Vec3::Z,
            detonate_at: None,
        }
    }

    #[test]
    fn steer_clamps_rotation_to_turn_rate_times_delta() {
        let velocity = Vec3::Z * 12.0;
        let steered = steer(velocity, Vec3::X, 2.0, 0.1);
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
    fn stall_detonates_after_window_without_progress() {
        let mut info = missile_info();
        let pos = Position { x: 0.0, y: 2.0, z: 0.0 };

        assert!(!tick_missile_stall(&mut info, &pos, 0.1, 1.0), "first tick arms");
        for _ in 0..9 {
            assert!(!tick_missile_stall(&mut info, &pos, 0.1, 1.0));
        }
        assert!(
            tick_missile_stall(&mut info, &pos, 0.1, 1.0),
            "window elapsed while pinned"
        );
    }

    #[test]
    fn stall_re_anchors_on_progress() {
        let mut info = missile_info();
        let start = Position { x: 0.0, y: 2.0, z: 0.0 };
        let moved = Position { x: 0.0, y: 2.0, z: 1.5 };

        assert!(!tick_missile_stall(&mut info, &start, 0.5, 1.0));
        assert!(!tick_missile_stall(&mut info, &moved, 0.5, 1.0), "progress re-anchors");
        assert_eq!(info.stall_timer, 1.0);
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
        assert!(angle <= (0.35_f32 * std::f32::consts::SQRT_2).atan() + 1e-3);
        assert!((bent.length() - far.length()).abs() < 1e-3, "range is preserved");

        let near = Vec3::Z * 0.5;
        let near_bent = weave_direction(near, 0.4, 1.0, 0.35);
        assert!(
            near.angle_between(near_bent) < 0.35 * (0.5 / WEAVE_FADE_DISTANCE) * std::f32::consts::SQRT_2 + 1e-3,
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
