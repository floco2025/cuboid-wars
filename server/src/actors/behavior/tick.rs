use std::collections::VecDeque;

use bevy::prelude::*;
use rand::{Rng, rng};

use crate::{
    actors::navigation::NavGraph,
    actors::{ActorGoal, ActorInfo, ActorMap},
    config::{ActorFireConfig, ActorKindServerConfig, ServerGameplayConfig},
    map::{ActorSpawnZone, MapConfig},
    network::broadcast_to_all,
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    constants::LEVEL_HEIGHT,
    map::{MapGeometry, level_for_y},
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, PlayerId, PlayerMarker, Position, SActorBeam, ServerMessage},
};

use super::{
    patrol::{fresh_patrol_goal, random_direction_time, random_patrol_intent},
    perception::visible_player,
    stall::{RETURN_RETRY_SECS, transition_stall_recovery},
    zone::{closest_point_in_rect, xz_distance_from_rect},
};

// Thin ECS shell: gathers plain-data inputs (configs, leash geometry, gated
// perception) and hands the entire per-actor decision to
// `tick_actor_behavior`, which owns every `ActorGoal` transition.
pub fn actors_behavior_system(
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
        // startup, so `expect_actor` cannot panic here.
        let actor_config = gameplay_config.expect_actor(&info.spawn_kind);
        let kind_config = server_gameplay_config.expect_actor(&info.spawn_kind);

        let zone = &map_config.actor_spawn_zones[info.spawn_zone_index];
        let zone_bounds = zone.xz_bounds(&map_geometry);
        let beyond_leash = xz_distance_from_rect(pos, zone_bounds) > active_leash(&info.goal, kind_config)
            || patrolling_off_zone_level(&info.goal, pos.y, zone.level);
        // Perception is skipped beyond the leash — the actor is heading home
        // regardless.
        let seen_player = if beyond_leash {
            None
        } else {
            visible_player(
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
        // A locked burst tracks its target by identity, not perception —
        // resolve the target's live position (None = died or logged off).
        let fire_target_position = if let ActorGoal::Fire { target, .. } = &info.goal {
            players
                .get(target)
                .filter(|player| player.logged_in && !player.is_dead())
                .and_then(|player| player_query.get(player.entity).ok())
                .map(|(_, target_pos)| *target_pos)
        } else {
            None
        };

        let inputs = BehaviorInputs {
            id: *id,
            pos: *pos,
            delta,
            beyond_leash,
            visible_player: seen_player,
            fire_target_position,
            zone,
            zone_bounds,
            nav_graph: &nav_graph,
            patrol_speed: actor_config.patrol_speed,
            kind_config,
        };
        if let ActorBehaviorOutcome::StartedBeam { target, duration_secs } =
            tick_actor_behavior(info, &inputs, &mut rng)
        {
            broadcast_to_all(
                &players,
                ServerMessage::ActorBeam(SActorBeam {
                    id: *id,
                    target,
                    duration_secs,
                }),
            );
        }
    }
}

// Everything the per-tick decision needs, as plain data — no ECS. Perception
// (needs queries) stays in the system shell; it passes `None` when
// `beyond_leash` (perception is skipped out there, exactly as before).
pub(super) struct BehaviorInputs<'a> {
    pub id: ActorId,
    pub pos: Position,
    pub delta: f32,
    pub beyond_leash: bool,
    pub visible_player: Option<(PlayerId, Position)>,
    // Live position of a locked `Fire` target, resolved by identity in the
    // shell; `None` for every other goal, or when the target is gone.
    pub fire_target_position: Option<Position>,
    pub zone: &'a ActorSpawnZone,
    pub zone_bounds: (f32, f32, f32, f32),
    pub nav_graph: &'a NavGraph,
    pub patrol_speed: f32,
    pub kind_config: &'a ActorKindServerConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ActorBehaviorOutcome {
    NoEvent,
    StartedBeam { target: PlayerId, duration_secs: f32 },
}

// Chase pursues on its own (longer) leash; everything else uses the patrol
// leash — so a predator can chase a fleeing player past its normal roam.
// The laser engagement states share the chase leash: Approach is the chase
// analogue, and a post-burst Flee shouldn't be instantly preempted into a
// Return (which is itself a retreat).
pub(super) fn active_leash(goal: &ActorGoal, kind_config: &ActorKindServerConfig) -> f32 {
    match goal {
        ActorGoal::Chase { .. } | ActorGoal::Approach { .. } | ActorGoal::Fire { .. } | ActorGoal::Flee { .. } => {
            kind_config.chase.leash
        }
        _ => kind_config.patrol.leash,
    }
}

// The leash distance is horizontal, so a patrol that wandered to a
// different FLOOR than its zone can hover directly above/below home and
// never trip it — squatting on the wrong level forever. Chase/Pursuit stay
// level-free (cross-level chases are legal).
pub(super) fn patrolling_off_zone_level(goal: &ActorGoal, pos_y: f32, zone_level: u8) -> bool {
    matches!(goal, ActorGoal::Patrol { .. }) && level_for_y(pos_y) != zone_level
}

pub(super) fn tick_actor_behavior(
    info: &mut ActorInfo,
    inputs: &BehaviorInputs<'_>,
    rng: &mut impl Rng,
) -> ActorBehaviorOutcome {
    let timers = tick_timers(info, inputs.delta);
    transition_active_fire_and_flee(info, inputs);
    transition_arrival(info, inputs);
    transition_leash(info, inputs, timers.return_grace);
    let outcome = transition_engagement(info, inputs, timers.reacquire_blocked);
    transition_stall_recovery(info, inputs, rng);
    transition_patrol_reroll(info, inputs, rng);
    outcome
}

struct TimerState {
    reacquire_blocked: bool,
    return_grace: bool,
}

fn tick_timers(info: &mut ActorInfo, delta: f32) -> TimerState {
    let timers = TimerState {
        reacquire_blocked: tick_chase_reacquire_timer(info, delta),
        return_grace: tick_return_retry_timer(info, delta),
    };
    info.fire_cooldown_timer = (info.fire_cooldown_timer - delta).max(0.0);
    if let ActorGoal::Patrol { ledge_escape_timer, .. } = &mut info.goal {
        *ledge_escape_timer = (*ledge_escape_timer - delta).max(0.0);
    }
    timers
}

fn transition_active_fire_and_flee(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>) {
    let burst_end_threat = if let ActorGoal::Fire {
        target_pos,
        remaining_secs,
        ..
    } = &mut info.goal
    {
        *remaining_secs -= inputs.delta;
        if let Some(live) = inputs.fire_target_position {
            *target_pos = live;
        }
        (*remaining_secs <= 0.0 || inputs.fire_target_position.is_none()).then_some(*target_pos)
    } else {
        None
    };
    if let Some(threat) = burst_end_threat {
        info.fire_cooldown_timer = fire_cooldown_secs(inputs);
        info.goal = ActorGoal::Flee { threat };
        info.stall_anchor = None;
    }
    if let ActorGoal::Flee { threat } = &mut info.goal {
        if let Some((_, live)) = inputs.visible_player {
            *threat = live;
        }
        if info.fire_cooldown_timer <= 0.0 {
            // Hidden long enough. Same-tick acquisition below may go straight
            // back to Approach (or Fire) if a player is still in sight.
            info.goal = fresh_patrol_goal();
        }
    }
}

fn transition_arrival(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>) {
    let pos = inputs.pos;
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
        ActorGoal::Patrol { .. }
        | ActorGoal::Chase { .. }
        | ActorGoal::Approach { .. }
        | ActorGoal::Fire { .. }
        | ActorGoal::Flee { .. } => false,
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
}

fn transition_leash(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, return_grace: bool) {
    if inputs.beyond_leash && !return_grace && !matches!(info.goal, ActorGoal::Return { .. }) {
        if matches!(info.goal, ActorGoal::Chase { .. } | ActorGoal::Approach { .. }) {
            info.chase_reacquire_timer = inputs.kind_config.senses.chase_reacquire_cooldown_secs;
        }
        // A burst can only be leash-preempted by knockback (a firing actor
        // doesn't move). Arm the re-fire cooldown so the abort still counts.
        if matches!(info.goal, ActorGoal::Fire { .. }) {
            info.fire_cooldown_timer = fire_cooldown_secs(inputs);
        }
        start_return(info, inputs);
    }
}

fn transition_engagement(
    info: &mut ActorInfo,
    inputs: &BehaviorInputs<'_>,
    reacquire_blocked: bool,
) -> ActorBehaviorOutcome {
    if inputs.beyond_leash {
        return ActorBehaviorOutcome::NoEvent;
    }
    if let Some(fire) = &inputs.kind_config.fire {
        transition_laser_engagement(info, inputs, fire, reacquire_blocked)
    } else {
        transition_contact_engagement(info, inputs, reacquire_blocked);
        ActorBehaviorOutcome::NoEvent
    }
}

fn transition_patrol_reroll(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, rng: &mut impl Rng) {
    if let ActorGoal::Patrol {
        intent,
        direction_timer,
        ..
    } = &mut info.goal
    {
        *direction_timer -= inputs.delta;
        if *direction_timer <= 0.0 {
            *direction_timer = random_direction_time(rng, inputs.kind_config);
            *intent = random_patrol_intent(rng, inputs.patrol_speed, inputs.kind_config.patrol.idle_probability);
        }
    }
}

// Contact kinds: (re)acquire a visible player whenever we're not
// mid-leash-cooldown and not returning. Deliberately independent of the
// current goal: a pursuit toward a stale last-seen spot must snap onto the
// player's *live* position the moment they reappear — the stale spot can sit
// through a wall, so it never arrives and never updates.
fn transition_contact_engagement(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, reacquire_blocked: bool) {
    let chase_target = if let ActorGoal::Chase { target } = &info.goal {
        Some(*target)
    } else {
        None
    };
    if let Some((_, target)) = inputs.visible_player
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

// Laser kinds: same acquisition shape as contact engagement, but the
// engagement is `Approach` (press to standoff) and — once in range with the
// cooldown lapsed — a locked `Fire` burst. A mid-burst or hiding actor
// ignores perception entirely: the burst target is locked and the flee has
// priority over re-engaging.
fn transition_laser_engagement(
    info: &mut ActorInfo,
    inputs: &BehaviorInputs<'_>,
    fire: &ActorFireConfig,
    reacquire_blocked: bool,
) -> ActorBehaviorOutcome {
    if matches!(info.goal, ActorGoal::Fire { .. } | ActorGoal::Flee { .. }) {
        return ActorBehaviorOutcome::NoEvent;
    }
    let approach_target_pos = if let ActorGoal::Approach { target_pos, .. } = &info.goal {
        Some(*target_pos)
    } else {
        None
    };
    if let Some((id, target_pos)) = inputs.visible_player
        && !reacquire_blocked
        && !matches!(info.goal, ActorGoal::Return { .. })
    {
        let in_range = inputs.pos.horizontal_distance_sq(&target_pos) <= fire.range * fire.range;
        if in_range && info.fire_cooldown_timer <= 0.0 {
            // Visibility already implies clear line of sight.
            info.goal = ActorGoal::Fire {
                target: id,
                target_pos,
                remaining_secs: fire.duration_secs,
            };
            info.stall_anchor = None;
            return ActorBehaviorOutcome::StartedBeam {
                target: id,
                duration_secs: fire.duration_secs,
            };
        } else {
            // A *fresh* approach starts its stall window from here.
            if approach_target_pos.is_none() {
                info.stall_anchor = None;
            }
            info.goal = ActorGoal::Approach { target: id, target_pos };
        }
    } else if let Some(last_seen) = approach_target_pos
        && inputs.visible_player.is_none()
    {
        // Lost sight mid-approach: identical demotion to a chase.
        info.goal = ActorGoal::Pursuit { last_seen };
        info.stall_anchor = None;
    }
    ActorBehaviorOutcome::NoEvent
}

// Guaranteed present while a burst-related transition runs: only laser kinds
// (fire config present) can ever be in `Fire`.
fn fire_cooldown_secs(inputs: &BehaviorInputs<'_>) -> f32 {
    inputs
        .kind_config
        .fire
        .as_ref()
        .map(|fire| fire.cooldown_secs)
        .expect("actor in a burst state has no fire config")
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
            "{}#{} (zone {}) has no usable return target from ({:.2},{:.2},{:.2}); patrolling {RETURN_RETRY_SECS}s before retry",
            info.spawn_kind, inputs.id.0, info.spawn_zone_index, pos.x, pos.y, pos.z
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
            "{}#{} returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): NO nav path — straight-line fallback to ({:.2},{:.2},{:.2}), dist {:.2}; may walk into a wall/barrier",
            info.spawn_kind,
            inputs.id.0,
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
            "{}#{} returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): first waypoint ({:.2},{:.2},{:.2}), dist {:.2}, {} waypoints",
            info.spawn_kind,
            inputs.id.0,
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
