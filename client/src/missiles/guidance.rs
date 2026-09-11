use super::{
    AirGraph, MissileFlight,
    steering::{
        closest_point_on_segment, lead_point, pick_clear_direction, steer_clear, sweep_clear, target_velocity_estimate,
        terminal_approach, travel_clear, weave_direction,
    },
};
use crate::constants::MISSILE_RADIUS;
use bevy::prelude::*;
use common::{
    config::MissilesConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{BarrierId, Position},
};
use std::collections::VecDeque;

// Net-displacement watchdog distance, 3D (missiles fly): a missile that
// doesn't displace this far within `stall_secs` is orbiting or wedged and
// self-detonates.
const MISSILE_STALL_PROGRESS_DISTANCE: f32 = 1.0;
// How long a picked avoidance direction is flown before re-deciding, so the
// missile doesn't dither between candidates every tick.
const MISSILE_AVOID_COMMIT_SECS: f32 = 0.25;
// How far ahead (in seconds of travel) a candidate direction must be clear.
const MISSILE_AVOID_LOOKAHEAD_SECS: f32 = 0.6;
// Waypoint following along the air-graph route.
const MISSILE_TURN_LOOKAHEAD_SECS: f32 = 0.35;
const MISSILE_PATH_RETRY_SECS: f32 = 0.5;
// How far (in grid cells) the target may drift before the route is replanned.
const MISSILE_PATH_TARGET_MOVED_CELLS: f32 = 1.0;

pub fn guide_missile(
    info: &mut MissileFlight,
    config: &MissilesConfig,
    air_graph: &AirGraph,
    carriers: &Carriers,
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    pos: Position,
    target: Option<Vec3>,
    velocity: Vec3,
    speed: f32,
    delta: f32,
) -> Vec3 {
    info.lifetime_timer -= delta;
    if info.lifetime_timer <= 0.0 {
        info.detonate_at = Some(pos);
        return velocity;
    }
    let mut velocity = velocity;
    if let Some(target) = target {
        let origin = Vec3::from(pos);
        velocity = guided_velocity(
            info, config, air_graph, carriers, world, open_kinds, origin, target, velocity, speed, delta,
        );
        if let Some(closest) = proximity_detonation(
            world,
            open_kinds,
            origin,
            velocity * delta,
            target,
            config.proximity_fuse_distance,
        ) {
            info.detonate_at = Some(closest.into());
        }
    } else {
        info.target = None;
    }
    if info
        .watchdog
        .tick_3d(&pos, delta, MISSILE_STALL_PROGRESS_DISTANCE, config.stall_secs)
    {
        info.detonate_at = Some(pos);
    }
    velocity
}

fn proximity_detonation(
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    origin: Vec3,
    travel: Vec3,
    target: Vec3,
    fuse_distance: f32,
) -> Option<Vec3> {
    let closest = closest_point_on_segment(origin, travel, target);
    (closest.distance_squared(target) <= fuse_distance * fuse_distance
        && world.attack_path_clear(closest, target, open_kinds)
        && travel_clear(world, open_kinds, origin, closest - origin, MISSILE_RADIUS))
    .then_some(closest)
}

fn guided_velocity(
    info: &mut MissileFlight,
    config: &MissilesConfig,
    air_graph: &AirGraph,
    carriers: &Carriers,
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    origin: Vec3,
    target: Vec3,
    velocity: Vec3,
    speed: f32,
    delta: f32,
) -> Vec3 {
    let target_velocity = target_velocity_estimate(info.last_target_center, target, delta);
    info.last_target_center = Some(target);
    let aim = lead_point(origin, target, target_velocity, speed);
    let objective = if terminal_approach(
        world,
        open_kinds,
        origin,
        target,
        MISSILE_RADIUS,
        config.proximity_fuse_distance,
    )
    .is_some()
    {
        info.path.clear();
        info.path_target = None;
        info.path_retry_timer = 0.0;
        let aim = if sweep_clear(world, open_kinds, origin, aim - origin, MISSILE_RADIUS) {
            aim
        } else {
            target
        };
        let woven = homing_objective(info, config, origin, aim);
        if sweep_clear(world, open_kinds, origin, woven, MISSILE_RADIUS) {
            woven
        } else {
            aim - origin
        }
    } else if let Some(direction) = route_objective(
        info,
        air_graph,
        carriers,
        world,
        open_kinds,
        origin,
        target,
        MISSILE_RADIUS,
        config.proximity_fuse_distance,
        delta,
    ) {
        direction
    } else {
        dodge_objective(info, world, open_kinds, origin, target, speed, delta)
    };
    let lookahead_secs = ((origin.distance(target) - config.proximity_fuse_distance).max(0.0)
        / speed.max(f32::EPSILON))
    .clamp(delta, MISSILE_TURN_LOOKAHEAD_SECS.max(delta));
    steer_clear(
        world,
        open_kinds,
        origin,
        velocity,
        objective,
        config.turn_radius,
        delta,
        lookahead_secs,
        MISSILE_RADIUS,
    )
}

// Clear sight line: fly at the (lead-pursuit) aim point with the cosmetic
// weave layered on.
fn homing_objective(info: &mut MissileFlight, config: &MissilesConfig, origin: Vec3, aim_point: Vec3) -> Vec3 {
    info.avoid_dir = None;
    let elapsed = config.lifetime_secs - info.lifetime_timer;
    weave_direction(aim_point - origin, elapsed, info.weave_phase, config.weave_strength)
}

// No line of sight: route through the 3D airspace graph. `None` when the
// graph has no route (sealed target, off-graph edge case).
#[expect(
    clippy::too_many_arguments,
    reason = "route following reads world, graph, and per-missile state"
)]
fn route_objective(
    info: &mut MissileFlight,
    air_graph: &AirGraph,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierId],
    origin: Vec3,
    target_center: Vec3,
    radius: f32,
    fuse_distance: f32,
    delta: f32,
) -> Option<Vec3> {
    info.path_retry_timer -= delta;
    let moved_threshold = MISSILE_PATH_TARGET_MOVED_CELLS * air_graph.cell_size();
    let target_moved = info
        .path_target
        .is_some_and(|prev| prev.distance_squared(target_center) > moved_threshold * moved_threshold);
    if target_moved
        || info.path_retry_timer <= 0.0
        || !route_clear(
            &info.path,
            origin,
            target_center,
            collision_world,
            open_kinds,
            radius,
            fuse_distance,
        )
    {
        info.path = air_graph
            .path(
                carriers,
                collision_world,
                open_kinds,
                origin,
                target_center,
                radius,
                fuse_distance,
            )
            .unwrap_or_default();
        info.path_target = Some(target_center);
        info.path_retry_timer = MISSILE_PATH_RETRY_SECS;
    }
    let found_route = !info.path.is_empty();
    advance_waypoints(&mut info.path, origin, collision_world, open_kinds, radius);
    if found_route && info.path.is_empty() {
        // An empty route reads as clear, so nothing would ask the graph again
        // for the whole retry window; a route that went unreachable asks on the
        // next tick, while a graph that found nothing waits the window out.
        info.path_retry_timer = 0.0;
    }
    let waypoint = info.path.front()?;
    info.avoid_dir = None;
    Some(*waypoint - origin)
}

// Last resort with no air route: local dodge fan. A committed direction is
// flown while its lookahead sweep stays clear; otherwise re-pick. Nothing
// clear: press at the target anyway — the stall watchdog detonates a wedged
// missile.
fn dodge_objective(
    info: &mut MissileFlight,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierId],
    origin: Vec3,
    aim_point: Vec3,
    missile_speed: f32,
    delta: f32,
) -> Vec3 {
    info.avoid_timer -= delta;
    let lookahead_distance = missile_speed * MISSILE_AVOID_LOOKAHEAD_SECS;
    let desired = (aim_point - origin).normalize_or_zero();
    let committed = info.avoid_dir.filter(|dir| {
        info.avoid_timer > 0.0
            && sweep_clear(
                collision_world,
                open_kinds,
                origin,
                *dir * lookahead_distance,
                MISSILE_RADIUS,
            )
    });
    let chosen = committed.or_else(|| {
        let picked = pick_clear_direction(
            collision_world,
            open_kinds,
            origin,
            desired,
            lookahead_distance,
            MISSILE_RADIUS,
        );
        info.avoid_dir = picked;
        info.avoid_timer = MISSILE_AVOID_COMMIT_SECS;
        picked
    });
    chosen.unwrap_or(desired)
}

fn route_clear(
    path: &VecDeque<Vec3>,
    origin: Vec3,
    target: Vec3,
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    radius: f32,
    fuse_distance: f32,
) -> bool {
    let mut previous = origin;
    path.iter().all(|point| {
        let clear = sweep_clear(world, open_kinds, previous, *point - previous, radius);
        previous = *point;
        clear
    }) && path
        .back()
        .is_none_or(|end| terminal_approach(world, open_kinds, *end, target, radius, fuse_distance).is_some())
}

fn advance_waypoints(
    path: &mut VecDeque<Vec3>,
    origin: Vec3,
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    radius: f32,
) {
    // Near a corner is not past it: skip only waypoints with a clear shortcut.
    if let Some(index) = path
        .iter()
        .rposition(|point| sweep_clear(world, open_kinds, origin, *point - origin, radius))
    {
        path.drain(..index);
    } else {
        path.clear();
    }
}

#[cfg(test)]
#[path = "tests/guidance.rs"]
mod tests;
