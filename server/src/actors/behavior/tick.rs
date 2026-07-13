use std::collections::VecDeque;

use bevy::prelude::*;
use rand::{rng, rngs::ThreadRng};

use crate::{
    actors::navigation::NavGraph,
    actors::{ActorGoal, ActorInfo, ActorMap},
    config::{ActorKindServerConfig, ServerGameplayConfig},
    map::{ActorSpawnZone, MapConfig},
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    constants::LEVEL_HEIGHT,
    map::{MapGeometry, compute_player_level},
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, PlayerId, PlayerMarker, Position},
};

use super::{
    patrol::{forced_patrol_goal, fresh_patrol_goal, random_direction_time, random_patrol_intent},
    perception::visible_player_position,
    zone::{closest_point_in_rect, xz_distance_from_rect},
};

// Thin ECS shell: gathers plain-data inputs (configs, leash geometry, gated
// perception) and hands the entire per-actor decision to
// `tick_actor_behavior`, which owns every `ActorGoal` transition.
pub fn actor_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    nav_graph: Res<NavGraph>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    query: Query<(&ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let mut rng = rng();

    for (id, pos) in &query {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        // Per-kind config: actor kinds are cross-validated against the map at
        // startup, so `validated_actor` cannot panic here.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_config = server_gameplay_config.validated_actor(&info.spawn_kind);

        let zone = &map_config.actor_spawn_zones[info.spawn_zone_index];
        let zone_bounds = zone.xz_bounds(&map_geometry);
        let beyond_leash = xz_distance_from_rect(pos, zone_bounds) > active_leash(&info.goal, kind_config)
            || patrolling_off_zone_level(&info.goal, pos.y, zone.level);
        // Perception is skipped beyond the leash — the actor is heading home
        // regardless.
        let visible_player = if beyond_leash {
            None
        } else {
            visible_player_position(
                pos,
                actor_config.eye_height(),
                kind_config.senses.horizontal_vision_range,
                kind_config.senses.vertical_vision_range,
                &players,
                &player_query,
                &collision_world,
                &gameplay_config,
            )
        };

        let inputs = BehaviorInputs {
            pos: *pos,
            delta,
            beyond_leash,
            visible_player,
            zone,
            zone_bounds,
            nav_graph: &nav_graph,
            patrol_speed: actor_config.patrol_speed,
            kind_config,
        };
        tick_actor_behavior(info, &inputs, &mut rng);
    }
}

// Net-displacement stall watchdog: a goal-directed (or moving-patrol) actor
// that fails to displace `STALL_PROGRESS_DISTANCE` meters within its goal's
// window is wedged against geometry and its goal's escape fires. Keyed on
// net displacement, not distance-to-goal, so an actor legitimately routing
// *around* a wall (moving without getting closer) isn't called stuck.
pub(super) const STALL_PROGRESS_DISTANCE: f32 = 0.4;
// Demoted (lost-sight) pursuit toward a stale last-seen spot.
pub(super) const PURSUIT_GIVEUP_NO_PROGRESS_SECS: f32 = 1.5;
// Active chase pinned while the player stays visible (e.g. diagonally over a
// ledge rim it can't path to). Longer than the pursuit window — a live fight
// displaces plenty; only a genuine pin sits still this long.
pub(super) const CHASE_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Wedged return-to-spawn (includes the straight-line nav fallback).
pub(super) const RETURN_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Moving patrol that can't move (wall-base perch, boxed in). Longer than
// `patrol.max_direction_secs` (3.5) so a normal re-roll cycle isn't called stuck.
pub(super) const PATROL_GIVEUP_NO_PROGRESS_SECS: f32 = 4.0;
// Leash-suppressed patrol window between return attempts.
pub(super) const RETURN_RETRY_SECS: f32 = 5.0;
// Ledge-unaware candidate window after a patrol stall.
pub(super) const PATROL_LEDGE_ESCAPE_SECS: f32 = 1.0;

// Everything the per-tick decision needs, as plain data — no ECS. Perception
// (needs queries) stays in the system shell; it passes `None` when
// `beyond_leash` (perception is skipped out there, exactly as before).
pub(super) struct BehaviorInputs<'a> {
    pub pos: Position,
    pub delta: f32,
    pub beyond_leash: bool,
    pub visible_player: Option<Position>,
    pub zone: &'a ActorSpawnZone,
    pub zone_bounds: (f32, f32, f32, f32),
    pub nav_graph: &'a NavGraph,
    pub patrol_speed: f32,
    pub kind_config: &'a ActorKindServerConfig,
}

// Chase pursues on its own (longer) leash; everything else uses the patrol
// leash — so a predator can chase a fleeing player past its normal roam.
pub(super) fn active_leash(goal: &ActorGoal, kind_config: &ActorKindServerConfig) -> f32 {
    match goal {
        ActorGoal::Chase { .. } => kind_config.chase.leash,
        _ => kind_config.patrol.leash,
    }
}

// The leash distance is horizontal, so a patrol that wandered to a
// different FLOOR than its zone can hover directly above/below home and
// never trip it — squatting on the wrong level forever. Chase/Pursuit stay
// level-free (cross-level chases are legal).
pub(super) fn patrolling_off_zone_level(goal: &ActorGoal, pos_y: f32, zone_level: u8) -> bool {
    matches!(goal, ActorGoal::Patrol { .. }) && compute_player_level(pos_y) != zone_level
}

// The whole per-actor decision for one tick: timers → arrival → leash →
// acquire/demote → stall watchdog → patrol re-roll. Pure over `ActorInfo` +
// inputs so every transition is unit-testable end-to-end.
pub(super) fn tick_actor_behavior(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, rng: &mut ThreadRng) {
    let pos = inputs.pos;
    let delta = inputs.delta;
    let reacquire_blocked = tick_chase_reacquire_timer(info, delta);
    let return_grace = tick_return_retry_timer(info, delta);
    if let ActorGoal::Patrol { ledge_escape_timer, .. } = &mut info.goal {
        *ledge_escape_timer = (*ledge_escape_timer - delta).max(0.0);
    }

    // Arrival — behavior owns it (movement never mutates the goal). A
    // pursuit that reaches the last-seen spot gives up to patrol; a return
    // advances its waypoints. Chases press and never arrive.
    let reached_distance = inputs.kind_config.navigation.go_to_reached_distance;
    let arrived = match &info.goal {
        ActorGoal::Pursuit { last_seen } => {
            pos.horizontal_distance_sq(last_seen) <= reached_distance * reached_distance
        }
        // Deliberately horizontal-only: the two waypoints of a ramp
        // transition share x/z (sloped cell + the opening above), so any
        // vertical gate here strands the actor mid-slope steering at a point
        // straight overhead. A fallen actor popping elevated waypoints is
        // instead recovered by the stall watchdog + retry, which re-plans
        // from its real position.
        ActorGoal::Return { next, .. } => pos.horizontal_distance_sq(next) <= reached_distance * reached_distance,
        ActorGoal::Patrol { .. } | ActorGoal::Chase { .. } => false,
    };
    if arrived {
        info.goal = match std::mem::replace(&mut info.goal, fresh_patrol_goal()) {
            ActorGoal::Return { mut path, .. } => match path.pop_front() {
                Some(next) => ActorGoal::Return { next, path },
                None => fresh_patrol_goal(),
            },
            _ => fresh_patrol_goal(),
        };
    }

    // Leash: strayed past the goal's leash from the spawn zone edge — walk
    // it home. `return_grace` (armed by a stalled return) suppresses the
    // re-arm so the actor patrols in place first and the next attempt starts
    // from a different position instead of livelocking on the same fallback
    // path.
    if inputs.beyond_leash && !return_grace && !matches!(info.goal, ActorGoal::Return { .. }) {
        if matches!(info.goal, ActorGoal::Chase { .. }) {
            info.chase_reacquire_timer = inputs.kind_config.senses.chase_reacquire_cooldown_secs;
        }
        start_return(info, inputs);
    }

    if !inputs.beyond_leash {
        let chase_target = if let ActorGoal::Chase { target } = &info.goal {
            Some(*target)
        } else {
            None
        };
        // (Re)acquire a visible player whenever we're not mid-leash-cooldown
        // and not returning. Deliberately independent of the current goal: a
        // pursuit toward a stale last-seen spot must snap onto the player's
        // *live* position the moment they reappear — the stale spot can sit
        // through a wall, so it never arrives and never updates.
        if let Some(target) = inputs.visible_player
            && !reacquire_blocked
            && !matches!(info.goal, ActorGoal::Return { .. })
        {
            // A *fresh* chase starts its stall window from here.
            if chase_target.is_none() {
                info.stall_anchor = None;
            }
            info.goal = ActorGoal::Chase { target };
        } else if let Some(last_seen) = chase_target
            && inputs.visible_player.is_none()
        {
            // Lost sight (e.g. the player jumped to another floor). Demote
            // to a one-shot walk toward the last-seen spot; the actor either
            // arrives there and gives up to patrol, or stalls out.
            info.goal = ActorGoal::Pursuit { last_seen };
            info.stall_anchor = None;
        }
    }

    // One watchdog site sees every goal — early exits here are exactly how
    // actors used to end up grinding walls forever.
    match stall_window(&info.goal, &pos, reached_distance) {
        None => info.stall_anchor = None,
        Some(window) => {
            if tick_stall(info, &pos, delta, window) {
                escape_stall(info, inputs, rng);
            }
        }
    }

    // Patrol heading re-roll on its own cadence. A goal set this tick with a
    // fresh (zero) timer re-rolls immediately; a forced escape set a full
    // timer and keeps its guaranteed-Moving intent.
    if let ActorGoal::Patrol {
        intent,
        direction_timer,
        ..
    } = &mut info.goal
    {
        *direction_timer -= delta;
        if *direction_timer <= 0.0 {
            *direction_timer = random_direction_time(rng, inputs.kind_config);
            *intent = random_patrol_intent(rng, inputs.patrol_speed, inputs.kind_config.patrol.idle_probability);
        }
    }
}

// Which stall window (if any) applies to the current goal. The vertical hold
// (camping under a ledge player) and intentional patrol idle are stationary
// on purpose — exempt.
pub(super) fn stall_window(goal: &ActorGoal, pos: &Position, reached_distance: f32) -> Option<f32> {
    match goal {
        ActorGoal::Patrol {
            intent: ActorMoveIntent::Idle,
            ..
        } => None,
        ActorGoal::Patrol { .. } => Some(PATROL_GIVEUP_NO_PROGRESS_SECS),
        ActorGoal::Chase { .. } => {
            if goal.chase_hold(pos, reached_distance) {
                None
            } else {
                Some(CHASE_GIVEUP_NO_PROGRESS_SECS)
            }
        }
        ActorGoal::Pursuit { .. } => Some(PURSUIT_GIVEUP_NO_PROGRESS_SECS),
        ActorGoal::Return { .. } => Some(RETURN_GIVEUP_NO_PROGRESS_SECS),
    }
}

// The goal is wedged against geometry — replace it with something that can
// move. Contact detonation is geometric and unaffected by any of this.
fn escape_stall(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, rng: &mut ThreadRng) {
    let pos = inputs.pos;
    info.stall_anchor = None;
    match &info.goal {
        ActorGoal::Chase { .. } => {
            // Pinned with the player still visible. Give up and genuinely
            // wander off — the reacquire cooldown stops next tick's
            // perception from re-pinning it.
            debug!(
                "{} (zone {}) chase stalled at ({:.2},{:.2},{:.2}); giving up for {:.1}s",
                info.spawn_kind,
                info.spawn_zone_index,
                pos.x,
                pos.y,
                pos.z,
                inputs.kind_config.senses.chase_reacquire_cooldown_secs
            );
            info.chase_reacquire_timer = inputs.kind_config.senses.chase_reacquire_cooldown_secs;
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Pursuit { .. } => {
            debug!(
                "{} (zone {}) abandoning unreachable last-seen spot at ({:.2},{:.2},{:.2}); resuming patrol",
                info.spawn_kind, info.spawn_zone_index, pos.x, pos.y, pos.z
            );
            info.goal = fresh_patrol_goal();
        }
        ActorGoal::Return { next, path } => {
            warn!(
                "{} (zone {}) return stalled at ({:.2},{:.2},{:.2}) heading to waypoint ({:.2},{:.2},{:.2}) with {} more; patrolling {RETURN_RETRY_SECS}s before retry",
                info.spawn_kind,
                info.spawn_zone_index,
                pos.x,
                pos.y,
                pos.z,
                next.x,
                next.y,
                next.z,
                path.len()
            );
            info.return_retry_timer = RETURN_RETRY_SECS;
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Patrol { .. } => {
            // Wall-base perch / boxed in: every strict candidate is rejected
            // and re-rolls can't change the candidate set. Briefly allow
            // risky steps — grazes and off-edge drops; a fall is recycled by
            // removal/respawn.
            debug!(
                "{} (zone {}) patrol stalled at ({:.2},{:.2},{:.2}); ledge-unaware escape for {PATROL_LEDGE_ESCAPE_SECS}s",
                info.spawn_kind, info.spawn_zone_index, pos.x, pos.y, pos.z
            );
            let mut goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
            if let ActorGoal::Patrol { ledge_escape_timer, .. } = &mut goal {
                *ledge_escape_timer = PATROL_LEDGE_ESCAPE_SECS;
            }
            info.goal = goal;
        }
    }
}

fn start_return(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>) {
    let pos = inputs.pos;
    let path = inputs.nav_graph.path_to_spawn_zone(&pos, inputs.zone);
    let used_straight_line_fallback = path.as_ref().is_none_or(VecDeque::is_empty);
    let mut path = path.unwrap_or_default();
    let next = path.pop_front().unwrap_or_else(|| {
        let mut fallback = closest_point_in_rect(&pos, inputs.zone_bounds);
        fallback.y = f32::from(inputs.zone.level) * LEVEL_HEIGHT;
        fallback
    });

    // A fallback target the actor already stands on (horizontally) gives the
    // return nothing to walk toward — arrival would pop it the same tick and
    // the wrong-level leash would re-arm next tick, hot-looping
    // patrol→return→arrive at 30 Hz. Skip the attempt and retry after the
    // grace window instead.
    let reached_distance = inputs.kind_config.navigation.go_to_reached_distance;
    if used_straight_line_fallback && pos.horizontal_distance_sq(&next) <= reached_distance * reached_distance {
        warn!(
            "{} (zone {}) has no usable return target from ({:.2},{:.2},{:.2}); patrolling {RETURN_RETRY_SECS}s before retry",
            info.spawn_kind, info.spawn_zone_index, pos.x, pos.y, pos.z
        );
        info.return_retry_timer = RETURN_RETRY_SECS;
        return;
    }

    // Diagnostic. A missing nav path (straight-line fallback) is the real
    // anomaly — it can pin an actor against a wall until the return stall
    // frees it — so it stays at `warn`. A found path is the normal, healthy
    // case, logged at `debug` so it doesn't spam.
    let target_distance = pos.horizontal_distance_sq(&next).sqrt();
    if used_straight_line_fallback {
        warn!(
            "{} returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): NO nav path — straight-line fallback to ({:.2},{:.2},{:.2}), dist {:.2}; may walk into a wall/barrier",
            info.spawn_kind,
            info.spawn_zone_index,
            inputs.zone.level,
            pos.x,
            pos.y,
            pos.z,
            next.x,
            next.y,
            next.z,
            target_distance
        );
    } else {
        debug!(
            "{} returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): first waypoint ({:.2},{:.2},{:.2}), dist {:.2}, {} waypoints",
            info.spawn_kind,
            info.spawn_zone_index,
            inputs.zone.level,
            pos.x,
            pos.y,
            pos.z,
            next.x,
            next.y,
            next.z,
            target_distance,
            path.len()
        );
    }

    info.goal = ActorGoal::Return { next, path };
    // Fresh stall window for the new return.
    info.stall_anchor = None;
}

// Advance the no-progress watchdog. Self-arms when unarmed; re-anchors and
// refills the window when the actor has displaced past
// `STALL_PROGRESS_DISTANCE` from its anchor; otherwise counts the window
// down. Returns true once it has failed to make that much headway for
// `window_secs` — i.e. it's wedged against an obstacle.
pub(super) fn tick_stall(info: &mut ActorInfo, pos: &Position, delta: f32, window_secs: f32) -> bool {
    let Some(anchor) = info.stall_anchor else {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    };
    if anchor.horizontal_distance_sq(pos) > STALL_PROGRESS_DISTANCE * STALL_PROGRESS_DISTANCE {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    }
    info.stall_timer -= delta;
    info.stall_timer <= 0.0
}

pub(super) fn tick_chase_reacquire_timer(info: &mut ActorInfo, delta: f32) -> bool {
    if info.chase_reacquire_timer <= 0.0 {
        return false;
    }
    info.chase_reacquire_timer = (info.chase_reacquire_timer - delta).max(0.0);
    info.chase_reacquire_timer > 0.0
}

pub(super) fn tick_return_retry_timer(info: &mut ActorInfo, delta: f32) -> bool {
    if info.return_retry_timer <= 0.0 {
        return false;
    }
    info.return_retry_timer = (info.return_retry_timer - delta).max(0.0);
    info.return_retry_timer > 0.0
}
