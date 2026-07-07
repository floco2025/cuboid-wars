use std::collections::VecDeque;

use bevy::prelude::*;
use rand::rngs::ThreadRng;

use crate::{
    actors::navigation::NavGraph,
    config::ActorKindServerConfig,
    resources::{ActorGoal, ActorInfo, ActorSpawnZone},
};
use common::{
    constants::LEVEL_HEIGHT,
    map::compute_player_level,
    protocol::{ActorMoveIntent, Position},
};

use super::{
    patrol::{forced_patrol_goal, fresh_patrol_goal, random_direction_time, random_patrol_intent},
    zone::closest_point_in_rect,
};

// Net-displacement stall watchdog: a goal-directed (or moving-patrol) actor
// that fails to displace `STALL_PROGRESS_DISTANCE` meters within its goal's
// window is wedged against geometry and its goal's escape fires. Keyed on
// net displacement, not distance-to-goal, so an actor legitimately routing
// *around* a wall (moving without getting closer) isn't called stuck.
const STALL_PROGRESS_DISTANCE: f32 = 0.4;
// Demoted (lost-sight) pursuit toward a stale last-seen spot.
const PURSUIT_GIVEUP_NO_PROGRESS_SECS: f32 = 1.5;
// Active chase pinned while the player stays visible (e.g. diagonally over a
// ledge rim it can't path to). Longer than the pursuit window — a live fight
// displaces plenty; only a genuine pin sits still this long.
const CHASE_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Wedged return-to-spawn (includes the straight-line nav fallback).
const RETURN_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Moving patrol that can't move (wall-base perch, boxed in). Longer than
// `patrol.max_direction_secs` (3.5) so a normal re-roll cycle isn't called stuck.
const PATROL_GIVEUP_NO_PROGRESS_SECS: f32 = 4.0;
// Leash-suppressed patrol window between return attempts.
const RETURN_RETRY_SECS: f32 = 5.0;
// Ledge-unaware candidate window after a patrol stall.
const PATROL_LEDGE_ESCAPE_SECS: f32 = 1.0;

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
fn stall_window(goal: &ActorGoal, pos: &Position, reached_distance: f32) -> Option<f32> {
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
fn tick_stall(info: &mut ActorInfo, pos: &Position, delta: f32, window_secs: f32) -> bool {
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

fn tick_chase_reacquire_timer(info: &mut ActorInfo, delta: f32) -> bool {
    if info.chase_reacquire_timer <= 0.0 {
        return false;
    }
    info.chase_reacquire_timer = (info.chase_reacquire_timer - delta).max(0.0);
    info.chase_reacquire_timer > 0.0
}

fn tick_return_retry_timer(info: &mut ActorInfo, delta: f32) -> bool {
    if info.return_retry_timer <= 0.0 {
        return false;
    }
    info.return_retry_timer = (info.return_retry_timer - delta).max(0.0);
    info.return_retry_timer > 0.0
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;

    use super::*;
    use crate::{
        config::ServerGameplayConfig,
        resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig},
    };
    use common::map_geometry::MapGeometry;

    fn actor_info(goal: ActorGoal) -> ActorInfo {
        ActorInfo::new(Entity::from_bits(1), 0, "mine_1".into(), goal)
    }

    fn moving_patrol() -> ActorGoal {
        ActorGoal::Patrol {
            intent: ActorMoveIntent::Moving {
                direction: 0.0,
                speed: 2.0,
            },
            direction_timer: 100.0,
            ledge_escape_timer: 0.0,
        }
    }

    fn nav_graph() -> NavGraph {
        let mut cells = CellGrid::new(1, 1);
        cells.rows[0][0].has_floor = true;
        let map_config = MapConfig {
            levels: vec![LevelGrid {
                cells,
                edges: EdgeGrid::new(1, 1),
                barrier_edges: EdgeGrid::new(1, 1),
            }],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            cookie_spawn_zones: Vec::new(),
            key_spawn_zones: Vec::new(),
            pressure_plates: Vec::new(),
        };
        NavGraph::new(map_config, MapGeometry::new(1, 1))
    }

    fn zone() -> ActorSpawnZone {
        ActorSpawnZone {
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: "mine_1".into(),
            count: 1,
        }
    }

    fn server_config() -> ServerGameplayConfig {
        ServerGameplayConfig::load_default().expect("default server gameplay config should load")
    }

    struct Fixture {
        nav: NavGraph,
        zone: ActorSpawnZone,
        config: ServerGameplayConfig,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                nav: nav_graph(),
                zone: zone(),
                config: server_config(),
            }
        }

        fn inputs(&self, pos: Position, visible_player: Option<Position>, beyond_leash: bool) -> BehaviorInputs<'_> {
            BehaviorInputs {
                pos,
                delta: 0.1,
                beyond_leash,
                visible_player,
                zone: &self.zone,
                zone_bounds: (-2.0, -2.0, 2.0, 2.0),
                nav_graph: &self.nav,
                patrol_speed: 2.0,
                kind_config: self.config.validated_actor("mine_1"),
            }
        }
    }

    fn tick(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>) {
        tick_actor_behavior(info, inputs, &mut rand::rng());
    }

    // ---- level awareness ---------------------------------------------------

    #[test]
    fn already_arrived_fallback_skips_return_and_arms_grace() {
        let fixture = Fixture::new();
        // Wrong level, horizontally inside the zone rect: the 1×1 single-level
        // nav graph can't path there, and the straight-line fallback lands at
        // the actor's own feet — a return with nothing to walk toward.
        let mut info = actor_info(moving_patrol());
        let pos = Position {
            x: 0.0,
            y: LEVEL_HEIGHT,
            z: 0.0,
        };

        tick(&mut info, &fixture.inputs(pos, None, true));

        assert!(
            matches!(info.goal, ActorGoal::Patrol { .. }),
            "a degenerate return must not start: {:?}",
            info.goal
        );
        assert!(
            info.return_retry_timer > 0.0,
            "grace must arm so the retry isn't a 30 Hz hot loop"
        );
    }

    #[test]
    fn patrol_off_zone_level_reads_as_beyond_leash() {
        use common::constants::LEVEL_HEIGHT;
        assert!(patrolling_off_zone_level(&moving_patrol(), LEVEL_HEIGHT, 0));
        assert!(!patrolling_off_zone_level(&moving_patrol(), 0.1, 0));
        let chase = ActorGoal::Chase {
            target: Position::default(),
        };
        assert!(
            !patrolling_off_zone_level(&chase, LEVEL_HEIGHT, 0),
            "cross-level chases stay legal"
        );
    }

    // ---- timers -----------------------------------------------------------

    #[test]
    fn chase_reacquire_timer_blocks_until_elapsed() {
        let mut info = actor_info(fresh_patrol_goal());
        info.chase_reacquire_timer = 1.0;

        assert!(tick_chase_reacquire_timer(&mut info, 0.25));
        assert_eq!(info.chase_reacquire_timer, 0.75);
        assert!(!tick_chase_reacquire_timer(&mut info, 0.75));
        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn return_retry_timer_grants_grace_until_elapsed() {
        let mut info = actor_info(fresh_patrol_goal());
        info.return_retry_timer = 1.0;

        assert!(tick_return_retry_timer(&mut info, 0.25));
        assert_eq!(info.return_retry_timer, 0.75);
        assert!(!tick_return_retry_timer(&mut info, 0.75));
        assert_eq!(info.return_retry_timer, 0.0);
    }

    // ---- stall watchdog primitives ----------------------------------------

    #[test]
    fn stall_self_arms_with_full_window() {
        let mut info = actor_info(fresh_patrol_goal());
        assert!(!tick_stall(&mut info, &Position::default(), 0.2, 2.0));
        assert_eq!(info.stall_anchor, Some(Position::default()));
        assert_eq!(info.stall_timer, 2.0);
    }

    #[test]
    fn stall_fires_after_no_progress_window() {
        let mut info = actor_info(fresh_patrol_goal());
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.1;
        assert!(tick_stall(&mut info, &Position::default(), 0.2, 1.5));
    }

    #[test]
    fn stall_progress_refills_window() {
        let mut info = actor_info(fresh_patrol_goal());
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.1;
        let moved = Position { x: 1.0, y: 0.0, z: 0.0 };
        assert!(!tick_stall(&mut info, &moved, 0.2, 3.0));
        assert_eq!(info.stall_timer, 3.0);
        assert_eq!(info.stall_anchor, Some(moved));
    }

    #[test]
    fn stall_holds_while_window_remains() {
        let mut info = actor_info(fresh_patrol_goal());
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 1.0;
        assert!(!tick_stall(&mut info, &Position::default(), 0.2, 1.5));
    }

    #[test]
    fn stall_window_classifies_goals() {
        let pos = Position::default();

        // Idle patrol: intentionally stationary.
        assert_eq!(stall_window(&fresh_patrol_goal(), &pos, 0.5), None);
        // Moving patrol counts.
        assert_eq!(
            stall_window(&moving_patrol(), &pos, 0.5),
            Some(PATROL_GIVEUP_NO_PROGRESS_SECS)
        );
        // Pressing chase.
        let chase = ActorGoal::Chase {
            target: Position { x: 5.0, y: 0.0, z: 0.0 },
        };
        assert_eq!(stall_window(&chase, &pos, 0.5), Some(CHASE_GIVEUP_NO_PROGRESS_SECS));
        // Holding under a ledge player: intentionally stationary.
        let holding = ActorGoal::Chase {
            target: Position { x: 0.3, y: 5.0, z: 0.0 },
        };
        assert_eq!(stall_window(&holding, &pos, 0.5), None);
        let pursuit = ActorGoal::Pursuit {
            last_seen: Position { x: 5.0, y: 0.0, z: 0.0 },
        };
        assert_eq!(stall_window(&pursuit, &pos, 0.5), Some(PURSUIT_GIVEUP_NO_PROGRESS_SECS));
        let returning = ActorGoal::Return {
            next: Position { x: 5.0, y: 0.0, z: 0.0 },
            path: VecDeque::new(),
        };
        assert_eq!(
            stall_window(&returning, &pos, 0.5),
            Some(RETURN_GIVEUP_NO_PROGRESS_SECS)
        );
    }

    // ---- end-to-end transitions -------------------------------------------

    #[test]
    fn acquisition_starts_fresh_chase_with_new_stall_window() {
        let fixture = Fixture::new();
        let mut info = actor_info(moving_patrol());
        info.stall_anchor = Some(Position { x: 9.0, y: 0.0, z: 9.0 });
        let target = Position { x: 5.0, y: 0.0, z: 5.0 };

        tick(&mut info, &fixture.inputs(Position::default(), Some(target), false));

        assert_eq!(info.goal, ActorGoal::Chase { target });
        // Fresh chase → the watchdog re-armed from the current position.
        assert_eq!(info.stall_anchor, Some(Position::default()));
        assert_eq!(info.stall_timer, CHASE_GIVEUP_NO_PROGRESS_SECS);
    }

    #[test]
    fn chase_retargets_live_player_keeping_stall_anchor() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Chase {
            target: Position { x: 4.0, y: 0.0, z: 4.0 },
        });
        let anchor = Position { x: 0.1, y: 0.0, z: 0.1 };
        info.stall_anchor = Some(anchor);
        info.stall_timer = 1.0;
        let moved_target = Position { x: 5.0, y: 0.0, z: 5.0 };

        tick(
            &mut info,
            &fixture.inputs(Position::default(), Some(moved_target), false),
        );

        assert_eq!(info.goal, ActorGoal::Chase { target: moved_target });
        // Retargeting an ongoing chase must NOT refresh the stall window —
        // that would let a pinned actor evade the watchdog forever.
        assert_eq!(info.stall_anchor, Some(anchor));
    }

    #[test]
    fn pursuit_snaps_to_reappeared_player() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Pursuit {
            last_seen: Position { x: 9.0, y: 0.0, z: 0.0 },
        });
        let live = Position { x: 5.0, y: 0.0, z: 5.0 };

        tick(&mut info, &fixture.inputs(Position::default(), Some(live), false));

        assert_eq!(info.goal, ActorGoal::Chase { target: live });
    }

    #[test]
    fn chase_demotes_to_pursuit_when_sight_is_lost() {
        let fixture = Fixture::new();
        let target = Position { x: 5.0, y: 0.0, z: 5.0 };
        let mut info = actor_info(ActorGoal::Chase { target });
        info.stall_anchor = Some(Position { x: 9.0, y: 0.0, z: 9.0 });

        tick(&mut info, &fixture.inputs(Position::default(), None, false));

        assert_eq!(info.goal, ActorGoal::Pursuit { last_seen: target });
        // Fresh window for the demoted pursuit.
        assert_eq!(info.stall_anchor, Some(Position::default()));
    }

    #[test]
    fn reacquire_cooldown_blocks_acquisition() {
        let fixture = Fixture::new();
        let mut info = actor_info(moving_patrol());
        info.chase_reacquire_timer = 5.0;

        tick(
            &mut info,
            &fixture.inputs(Position::default(), Some(Position { x: 5.0, y: 0.0, z: 5.0 }), false),
        );

        assert!(matches!(info.goal, ActorGoal::Patrol { .. }));
    }

    #[test]
    fn returning_actor_ignores_visible_players() {
        let fixture = Fixture::new();
        let next = Position { x: 5.0, y: 0.0, z: 0.0 };
        let mut info = actor_info(ActorGoal::Return {
            next,
            path: VecDeque::new(),
        });

        tick(
            &mut info,
            &fixture.inputs(Position::default(), Some(Position { x: 1.0, y: 0.0, z: 1.0 }), false),
        );

        assert_eq!(
            info.goal,
            ActorGoal::Return {
                next,
                path: VecDeque::new()
            }
        );
    }

    #[test]
    fn pursuit_arrival_resumes_patrol_with_same_tick_reroll() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Pursuit {
            last_seen: Position { x: 0.1, y: 0.0, z: 0.0 },
        });

        tick(&mut info, &fixture.inputs(Position::default(), None, false));

        // mine_1 has idle_probability 0, so the same-tick re-roll must yield
        // a Moving patrol at patrol speed with a live direction timer.
        let ActorGoal::Patrol {
            intent,
            direction_timer,
            ..
        } = info.goal
        else {
            panic!("expected patrol after pursuit arrival, got {:?}", info.goal);
        };
        assert!(matches!(intent, ActorMoveIntent::Moving { speed, .. } if speed == 2.0));
        assert!(direction_timer > 0.0);
    }

    #[test]
    fn return_arrival_advances_waypoints_then_completes() {
        let fixture = Fixture::new();
        let second = Position { x: 5.0, y: 0.0, z: 0.0 };
        let mut info = actor_info(ActorGoal::Return {
            next: Position { x: 0.1, y: 0.0, z: 0.0 },
            path: VecDeque::from([second]),
        });

        tick(&mut info, &fixture.inputs(Position::default(), None, false));
        assert_eq!(
            info.goal,
            ActorGoal::Return {
                next: second,
                path: VecDeque::new()
            }
        );

        // Reaching the final waypoint completes the return into patrol.
        let mut info = actor_info(ActorGoal::Return {
            next: Position { x: 0.1, y: 0.0, z: 0.0 },
            path: VecDeque::new(),
        });
        tick(&mut info, &fixture.inputs(Position::default(), None, false));
        assert!(matches!(info.goal, ActorGoal::Patrol { .. }));
    }

    #[test]
    fn return_completion_allows_same_tick_acquisition() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Return {
            next: Position { x: 0.1, y: 0.0, z: 0.0 },
            path: VecDeque::new(),
        });
        let live = Position { x: 3.0, y: 0.0, z: 3.0 };

        tick(&mut info, &fixture.inputs(Position::default(), Some(live), false));

        assert_eq!(info.goal, ActorGoal::Chase { target: live });
    }

    #[test]
    fn leash_breach_starts_return_and_arms_cooldown_for_chases() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Chase {
            target: Position {
                x: 50.0,
                y: 0.0,
                z: 0.0,
            },
        });
        let pos = Position {
            x: 40.0,
            y: 0.0,
            z: 0.0,
        };

        tick(&mut info, &fixture.inputs(pos, None, true));

        assert!(matches!(info.goal, ActorGoal::Return { .. }));
        assert_eq!(
            info.chase_reacquire_timer,
            fixture
                .config
                .validated_actor("mine_1")
                .senses
                .chase_reacquire_cooldown_secs
        );
    }

    #[test]
    fn leash_breach_from_patrol_does_not_arm_cooldown() {
        let fixture = Fixture::new();
        let mut info = actor_info(moving_patrol());
        let pos = Position {
            x: 40.0,
            y: 0.0,
            z: 0.0,
        };

        tick(&mut info, &fixture.inputs(pos, None, true));

        assert!(matches!(info.goal, ActorGoal::Return { .. }));
        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn return_retry_grace_suppresses_leash_rearm() {
        let fixture = Fixture::new();
        let mut info = actor_info(moving_patrol());
        info.return_retry_timer = 5.0;
        let pos = Position {
            x: 40.0,
            y: 0.0,
            z: 0.0,
        };

        tick(&mut info, &fixture.inputs(pos, None, true));

        assert!(matches!(info.goal, ActorGoal::Patrol { .. }));
    }

    #[test]
    fn stalled_chase_gives_up_and_arms_cooldown() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Chase {
            target: Position { x: 5.0, y: 0.0, z: 0.0 },
        });
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.05;

        tick(
            &mut info,
            &fixture.inputs(Position::default(), Some(Position { x: 5.0, y: 0.0, z: 0.0 }), false),
        );

        let ActorGoal::Patrol { intent, .. } = info.goal else {
            panic!("expected forced patrol after chase stall, got {:?}", info.goal);
        };
        assert!(matches!(intent, ActorMoveIntent::Moving { .. }));
        assert!(info.chase_reacquire_timer > 0.0);
        assert_eq!(info.stall_anchor, None);
    }

    #[test]
    fn stalled_return_arms_retry_grace() {
        let fixture = Fixture::new();
        let mut info = actor_info(ActorGoal::Return {
            next: Position { x: 5.0, y: 0.0, z: 0.0 },
            path: VecDeque::from([Position { x: 6.0, y: 0.0, z: 0.0 }]),
        });
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.05;

        tick(&mut info, &fixture.inputs(Position::default(), None, false));

        assert!(matches!(
            info.goal,
            ActorGoal::Patrol {
                intent: ActorMoveIntent::Moving { .. },
                ..
            }
        ));
        assert_eq!(info.return_retry_timer, RETURN_RETRY_SECS);
    }

    #[test]
    fn stalled_patrol_arms_ledge_escape_window() {
        let fixture = Fixture::new();
        let mut info = actor_info(moving_patrol());
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.05;

        tick(&mut info, &fixture.inputs(Position::default(), None, false));

        let ActorGoal::Patrol {
            intent,
            ledge_escape_timer,
            ..
        } = info.goal
        else {
            panic!("expected patrol after patrol stall, got {:?}", info.goal);
        };
        assert!(matches!(intent, ActorMoveIntent::Moving { .. }));
        // The escape window, minus nothing — armed after this tick's timer
        // decrement already happened.
        assert_eq!(ledge_escape_timer, PATROL_LEDGE_ESCAPE_SECS);
    }

    #[test]
    fn chase_hold_is_stall_exempt() {
        let fixture = Fixture::new();
        // Player on a ledge directly above, within horizontal reach.
        let target = Position { x: 0.3, y: 5.0, z: 0.0 };
        let mut info = actor_info(ActorGoal::Chase { target });
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.05;

        tick(&mut info, &fixture.inputs(Position::default(), Some(target), false));

        assert_eq!(info.goal, ActorGoal::Chase { target });
        assert_eq!(info.stall_anchor, None);
    }

    #[test]
    fn patrol_reroll_waits_for_direction_timer() {
        let fixture = Fixture::new();
        let intent = ActorMoveIntent::Moving {
            direction: 1.0,
            speed: 2.0,
        };
        let mut info = actor_info(ActorGoal::Patrol {
            intent,
            direction_timer: 1.0,
            ledge_escape_timer: 0.0,
        });

        tick(&mut info, &fixture.inputs(Position::default(), None, false));

        // Timer still live → the heading is untouched (deterministic).
        let ActorGoal::Patrol {
            intent: after,
            direction_timer,
            ..
        } = info.goal
        else {
            panic!("expected patrol, got {:?}", info.goal);
        };
        assert_eq!(after, intent);
        assert!((direction_timer - 0.9).abs() < 1e-4);
    }
}
