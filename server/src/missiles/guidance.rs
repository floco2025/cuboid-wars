use bevy::{ecs::system::SystemParam, prelude::*};

use super::steering::{
    closest_point_on_segment, lead_point, pick_clear_direction, steer, sweep_clear, target_velocity_estimate,
    weave_direction,
};
use crate::{
    actors::ActorMap,
    config::{MissilesServerConfig, ServerGameplayConfig},
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
// Waypoint following along the air-graph route.
const MISSILE_WAYPOINT_REACH_DISTANCE: f32 = 1.7;
const MISSILE_PATH_RETRY_SECS: f32 = 0.5;

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
                homing_objective(info, &config, origin, aim_point)
            } else if let Some(dir) = route_objective(
                info,
                &params.air_graph,
                &params.collision_world,
                &params.open_barrier_kinds.0,
                origin,
                target_center,
                config.radius,
                delta,
            ) {
                dir
            } else {
                dodge_objective(
                    info,
                    &params.collision_world,
                    &params.open_barrier_kinds.0,
                    origin,
                    aim_point,
                    &config,
                    delta,
                )
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

// Clear sight line: fly at the (lead-pursuit) aim point with the cosmetic
// weave layered on.
fn homing_objective(info: &mut MissileInfo, config: &MissilesServerConfig, origin: Vec3, aim_point: Vec3) -> Vec3 {
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
    info: &mut MissileInfo,
    air_graph: &AirGraph,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    target_center: Vec3,
    radius: f32,
    delta: f32,
) -> Option<Vec3> {
    info.path_retry_timer -= delta;
    let target_moved = info
        .path_target
        .is_some_and(|prev| prev.distance_squared(target_center) > GRID_CELL_SIZE * GRID_CELL_SIZE);
    if info.path.is_empty() || target_moved || info.path_retry_timer <= 0.0 {
        match air_graph.path(origin, target_center) {
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
    advance_waypoints(&mut info.path, origin, collision_world, open_kinds, radius);
    let waypoint = info.path.front()?;
    info.avoid_dir = None;
    Some(*waypoint - origin)
}

// Last resort with no air route: local dodge fan. A committed direction is
// flown while its lookahead sweep stays clear; otherwise re-pick. Nothing
// clear: press at the target anyway — the stall watchdog detonates a wedged
// missile.
fn dodge_objective(
    info: &mut MissileInfo,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    aim_point: Vec3,
    config: &MissilesServerConfig,
    delta: f32,
) -> Vec3 {
    info.avoid_timer -= delta;
    let lookahead = config.speed * MISSILE_AVOID_LOOKAHEAD_SECS;
    let desired = (aim_point - origin).normalize_or_zero();
    let committed = info.avoid_dir.filter(|dir| {
        info.avoid_timer > 0.0 && sweep_clear(collision_world, open_kinds, origin, *dir * lookahead, config.radius)
    });
    let chosen = committed.or_else(|| {
        let picked = pick_clear_direction(collision_world, open_kinds, origin, desired, lookahead, config.radius);
        info.avoid_dir = picked;
        info.avoid_timer = MISSILE_AVOID_COMMIT_SECS;
        picked
    });
    chosen.unwrap_or(desired)
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
    use common::protocol::PlayerId;

    fn missile_info() -> MissileInfo {
        MissileInfo::new(Entity::PLACEHOLDER, PlayerId(1), None, Vec3::Z, 0.0, 10.0, 2.0)
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
}
