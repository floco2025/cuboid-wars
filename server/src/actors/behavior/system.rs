use bevy::prelude::*;
use rand::rng;

use crate::{
    actors::navigation::NavGraph,
    config::ServerGameplayConfig,
    resources::{ActorMap, MapConfig, PlayerMap},
};
use common::{
    config::GameplayConfig,
    map_geometry::MapGeometry,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, PlayerId, PlayerMarker, Position},
};

use super::{
    patrol::{random_direction_time, random_patrol_intent, random_patrol_move_intent},
    perception::visible_player_position,
    zone::{closest_point_in_rect, xz_distance_from_rect},
};
use crate::actors::chase_target_within_reach;
use common::protocol::ActorMoveIntent;

// Net-displacement stall watchdog: a goal-directed (or moving-patrol) actor
// that fails to displace `STALL_PROGRESS_DISTANCE` meters within its state's
// window is wedged against geometry and its state's escape fires. Keyed on
// net displacement, not distance-to-goal, so an actor legitimately routing
// *around* a wall (moving without getting closer) isn't called stuck.
const STALL_PROGRESS_DISTANCE: f32 = 0.4;
// Demoted (lost-sight) pursuit toward a stale last-known spot.
const CHASE_GIVEUP_NO_PROGRESS_SECS: f32 = 1.5;
// Active chase pinned while the player stays visible (e.g. diagonally over a
// ledge rim it can't path to). Longer than the demoted window — a live fight
// displaces plenty; only a genuine pin sits still this long.
const ACTIVE_CHASE_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Wedged return-to-spawn (includes the straight-line nav fallback).
const RETURN_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
// Moving patrol that can't move (wall-base perch, boxed in). Longer than
// `patrol.max_direction_secs` (3.5) so a normal re-roll cycle isn't called stuck.
const PATROL_GIVEUP_NO_PROGRESS_SECS: f32 = 4.0;
// Leash-suppressed patrol window between return attempts.
const RETURN_RETRY_SECS: f32 = 5.0;
// Ledge-unaware candidate window after a patrol stall.
const PATROL_LEDGE_ESCAPE_SECS: f32 = 1.0;

// Which stall window (if any) applies to the actor's current state.
#[derive(Debug, PartialEq, Eq)]
enum StallGoal {
    Exempt,
    ActiveChase,
    DemotedPursuit,
    Return,
    Patrol,
}

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
        let kind_server_config = server_gameplay_config.validated_actor(&info.spawn_kind);
        let chase_reacquire_blocked = tick_chase_reacquire_timer(info, delta);
        tick_patrol_ledge_escape_timer(info, delta);
        let return_grace = tick_return_retry_timer(info, delta);

        // Leash: if the actor has strayed past the configured leash distance
        // from the nearest edge of its spawn zone, override everything else,
        // start the chase reacquire cooldown when applicable, and walk it
        // back. The patrol and chase leashes are independent so a predator
        // can pursue a fleeing player past its normal roam. `return_grace`
        // (armed by a stalled return) suppresses the re-arm so the actor
        // patrols in place first and the next attempt starts from a different
        // position instead of livelocking on the same fallback path.
        let zone_bounds = map_config.actor_spawn_zones[info.spawn_zone_index].xz_bounds(&map_geometry);
        let leash = if info.go_to_position_is_chase {
            kind_server_config.chase.leash
        } else {
            kind_server_config.patrol.leash
        };
        let beyond_leash = xz_distance_from_rect(pos, zone_bounds) > leash;
        if beyond_leash && !return_grace {
            if !info.is_returning_to_spawn {
                start_chase_reacquire_cooldown_if_chasing(
                    info,
                    kind_server_config.senses.chase_reacquire_cooldown_secs,
                );
                set_return_path_to_spawn_zone(
                    info,
                    pos,
                    &map_config.actor_spawn_zones[info.spawn_zone_index],
                    &nav_graph,
                    zone_bounds,
                );
            }
            info.go_to_position_is_chase = false;
        }

        if !beyond_leash {
            let visible_player = visible_player_position(
                pos,
                actor_config.eye_height(),
                kind_server_config.senses.horizontal_vision_range,
                kind_server_config.senses.vertical_vision_range,
                &players,
                &player_query,
                &collision_world,
                &gameplay_config,
            );

            // (Re)acquire a visible player whenever we're not mid-leash-cooldown and
            // not actively returning to spawn. Deliberately independent of the
            // current go-to: a chase demoted to a one-shot walk toward the player's
            // last-known spot (lost sight, below) must snap onto the player's *live*
            // position the moment they reappear. Otherwise the actor marches to that
            // stale spot — which can sit through a wall, so it never arrives and
            // never updates — even with the player standing right next to it.
            if ready_to_acquire_player(chase_reacquire_blocked, info.is_returning_to_spawn)
                && let Some(target_pos) = visible_player
            {
                // A *fresh* chase starts its stall window from here.
                if !info.go_to_position_is_chase {
                    info.stall_anchor = None;
                }
                info.go_to_position = Some(target_pos);
                info.go_to_position_is_chase = true;
            } else if lost_chase_target(info.go_to_position_is_chase, visible_player.is_some()) {
                // Lost sight of the chased player (it left vision — e.g. jumped to
                // another floor). Demote the chase to a one-shot go-to toward the
                // last-known spot: the actor walks there and then gives up to patrol
                // via the normal arrival path, instead of holding on a stale target
                // forever (arrival-hold only applies while `go_to_position_is_chase`).
                info.go_to_position_is_chase = false;
                // Fresh stall window for the demoted pursuit.
                info.stall_anchor = None;
            }
        }

        // One watchdog site sees every state — the old early-`continue`s made
        // active chases and returns structurally invisible to stall detection,
        // which is exactly how actors ended up grinding walls forever.
        match stall_goal(info, pos, kind_server_config.navigation.go_to_reached_distance) {
            StallGoal::Exempt => info.stall_anchor = None,
            StallGoal::ActiveChase => {
                if tick_stall(info, pos, delta, ACTIVE_CHASE_GIVEUP_NO_PROGRESS_SECS) {
                    debug!(
                        "actor chase stalled at ({:.2},{:.2},{:.2}); giving up for {:.1}s",
                        pos.x, pos.y, pos.z, kind_server_config.senses.chase_reacquire_cooldown_secs
                    );
                    abandon_stalled_chase(info, kind_server_config.senses.chase_reacquire_cooldown_secs);
                    force_fresh_patrol(info, &mut rng, actor_config.patrol_speed, kind_server_config);
                }
            }
            StallGoal::DemotedPursuit => {
                if tick_stall(info, pos, delta, CHASE_GIVEUP_NO_PROGRESS_SECS) {
                    debug!(
                        "actor abandoning unreachable last-known spot at ({:.2},{:.2},{:.2}); resuming patrol",
                        pos.x, pos.y, pos.z
                    );
                    info.go_to_position = None;
                    info.stall_anchor = None;
                }
            }
            StallGoal::Return => {
                if tick_stall(info, pos, delta, RETURN_GIVEUP_NO_PROGRESS_SECS) {
                    warn!(
                        "actor return stalled at ({:.2},{:.2},{:.2}); patrolling {RETURN_RETRY_SECS}s before retry",
                        pos.x, pos.y, pos.z
                    );
                    abandon_stalled_return(info);
                    force_fresh_patrol(info, &mut rng, actor_config.patrol_speed, kind_server_config);
                }
            }
            StallGoal::Patrol => {
                if tick_stall(info, pos, delta, PATROL_GIVEUP_NO_PROGRESS_SECS) {
                    debug!(
                        "actor patrol stalled at ({:.2},{:.2},{:.2}); ledge-unaware escape for {PATROL_LEDGE_ESCAPE_SECS}s",
                        pos.x, pos.y, pos.z
                    );
                    info.patrol_ledge_escape_timer = PATROL_LEDGE_ESCAPE_SECS;
                    info.stall_anchor = None;
                    force_fresh_patrol(info, &mut rng, actor_config.patrol_speed, kind_server_config);
                }
            }
        }

        if info.go_to_position.is_some() {
            continue;
        }
        info.direction_timer -= delta;
        if info.direction_timer > 0.0 {
            continue;
        }

        info.direction_timer = random_direction_time(&mut rng, kind_server_config);
        info.patrol_intent = random_patrol_intent(
            &mut rng,
            actor_config.patrol_speed,
            kind_server_config.patrol.idle_probability,
        );
    }
}

// Classify which stall window applies to the actor's current state. The
// vertical hold (camping under a ledge player) and intentional patrol idle
// are stationary on purpose — exempt.
fn stall_goal(info: &crate::resources::ActorInfo, pos: &Position, reached_distance: f32) -> StallGoal {
    if info.go_to_position.is_some() {
        if info.is_returning_to_spawn {
            return StallGoal::Return;
        }
        if info.go_to_position_is_chase {
            if chase_target_within_reach(info, pos, reached_distance) {
                return StallGoal::Exempt;
            }
            return StallGoal::ActiveChase;
        }
        return StallGoal::DemotedPursuit;
    }
    if !info.is_returning_to_spawn && matches!(info.patrol_intent, ActorMoveIntent::Moving { .. }) {
        return StallGoal::Patrol;
    }
    StallGoal::Exempt
}

// A stalled active chase goes straight to patrol with the reacquire cooldown
// armed — demoting it would just grind another window toward the same
// unreachable spot, and without the cooldown the next tick's perception
// re-pins it. Contact detonation is geometric and unaffected.
fn abandon_stalled_chase(info: &mut crate::resources::ActorInfo, chase_reacquire_cooldown: f32) {
    info.go_to_position = None;
    info.go_to_position_is_chase = false;
    info.chase_reacquire_timer = chase_reacquire_cooldown;
    info.stall_anchor = None;
}

fn abandon_stalled_return(info: &mut crate::resources::ActorInfo) {
    info.go_to_position = None;
    info.return_path.clear();
    info.is_returning_to_spawn = false;
    info.return_retry_timer = RETURN_RETRY_SECS;
    info.stall_anchor = None;
}

// After a stall the actor must actually move — bypass `idle_probability`.
fn force_fresh_patrol(
    info: &mut crate::resources::ActorInfo,
    rng: &mut rand::rngs::ThreadRng,
    patrol_speed: f32,
    kind_server_config: &crate::config::ActorKindServerConfig,
) {
    info.patrol_intent = random_patrol_move_intent(rng, patrol_speed);
    info.direction_timer = random_direction_time(rng, kind_server_config);
}

// A chasing actor that can no longer see the player should stop treating the
// stale last-known position as a live chase (demote it to a one-shot go-to so
// it walks there and then gives up to patrol).
fn lost_chase_target(is_chase: bool, player_visible: bool) -> bool {
    is_chase && !player_visible
}

// Whether the actor should (re)acquire a visible player this tick. Independent
// of the current go-to on purpose: a chase demoted to a stale last-known spot
// must re-acquire when the player reappears, not finish walking to the stale
// spot first. Only a leash cooldown or an in-progress return-to-spawn defers it.
fn ready_to_acquire_player(reacquire_blocked: bool, returning_to_spawn: bool) -> bool {
    !reacquire_blocked && !returning_to_spawn
}

// Advance the no-progress watchdog. Self-arms when unarmed; re-anchors and
// refills the window when the actor has displaced past
// `STALL_PROGRESS_DISTANCE` from its anchor; otherwise counts the window
// down. Returns true once it has failed to make that much headway for
// `window_secs` — i.e. it's wedged against an obstacle.
fn tick_stall(info: &mut crate::resources::ActorInfo, pos: &Position, delta: f32, window_secs: f32) -> bool {
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

fn tick_return_retry_timer(info: &mut crate::resources::ActorInfo, delta: f32) -> bool {
    if info.return_retry_timer <= 0.0 {
        return false;
    }
    info.return_retry_timer = (info.return_retry_timer - delta).max(0.0);
    info.return_retry_timer > 0.0
}

fn tick_patrol_ledge_escape_timer(info: &mut crate::resources::ActorInfo, delta: f32) {
    info.patrol_ledge_escape_timer = (info.patrol_ledge_escape_timer - delta).max(0.0);
}

fn tick_chase_reacquire_timer(info: &mut crate::resources::ActorInfo, delta: f32) -> bool {
    if info.chase_reacquire_timer <= 0.0 {
        return false;
    }
    info.chase_reacquire_timer = (info.chase_reacquire_timer - delta).max(0.0);
    info.chase_reacquire_timer > 0.0
}

fn start_chase_reacquire_cooldown_if_chasing(info: &mut crate::resources::ActorInfo, chase_reacquire_cooldown: f32) {
    if info.go_to_position_is_chase {
        info.chase_reacquire_timer = chase_reacquire_cooldown;
    }
}

fn set_return_path_to_spawn_zone(
    info: &mut crate::resources::ActorInfo,
    pos: &Position,
    zone: &crate::resources::ActorSpawnZone,
    nav_graph: &NavGraph,
    zone_bounds: (f32, f32, f32, f32),
) {
    let path = nav_graph.path_to_spawn_zone(pos, zone);
    let used_straight_line_fallback = path.as_ref().is_none_or(std::collections::VecDeque::is_empty);
    info.return_path = path.unwrap_or_default();
    info.go_to_position = info
        .return_path
        .pop_front()
        .or_else(|| Some(closest_point_in_rect(pos, zone_bounds)));

    // Diagnostic. A missing nav path (straight-line fallback) is the real
    // anomaly — it can pin an actor against a wall — so it stays at `warn`. A
    // found path is the normal, healthy case, logged at `debug` so it doesn't
    // spam (enable with `RUST_LOG=server::actors::behavior=debug`).
    if let Some(target) = info.go_to_position {
        let target_distance = pos.horizontal_distance_sq(&target).sqrt();
        if used_straight_line_fallback {
            warn!(
                "actor returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): NO nav path — straight-line fallback to ({:.2},{:.2},{:.2}), dist {:.2}; may walk into a wall/barrier",
                zone.kind, zone.level, pos.x, pos.y, pos.z, target.x, target.y, target.z, target_distance
            );
        } else {
            debug!(
                "actor returning to spawn zone {} (level {}) from ({:.2},{:.2},{:.2}): first waypoint ({:.2},{:.2},{:.2}), dist {:.2}, {} waypoints",
                zone.kind,
                zone.level,
                pos.x,
                pos.y,
                pos.z,
                target.x,
                target.y,
                target.z,
                target_distance,
                info.return_path.len()
            );
        }
    }

    info.is_returning_to_spawn = true;
    // Fresh stall window for the new return.
    info.stall_anchor = None;
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use common::protocol::ActorMoveIntent;

    use super::*;
    use crate::resources::ActorInfo;

    fn actor_info(chase_reacquire_timer: f32) -> ActorInfo {
        ActorInfo {
            entity: Entity::from_bits(1),
            spawn_zone_index: 0,
            spawn_kind: "mine_1".into(),
            direction_timer: 0.0,
            patrol_intent: ActorMoveIntent::Idle,
            go_to_position: None,
            go_to_position_is_chase: false,
            is_returning_to_spawn: false,
            return_path: Default::default(),
            chase_reacquire_timer,
            stall_anchor: None,
            stall_timer: 0.0,
            return_retry_timer: 0.0,
            patrol_ledge_escape_timer: 0.0,
            committed_direction: None,
            commit_secs_left: 0.0,
            last_damager: None,
        }
    }

    #[test]
    fn chase_reacquire_timer_blocks_until_elapsed() {
        let mut info = actor_info(1.0);

        assert!(tick_chase_reacquire_timer(&mut info, 0.25));
        assert_eq!(info.chase_reacquire_timer, 0.75);
        assert!(!tick_chase_reacquire_timer(&mut info, 0.75));
        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn interrupted_chase_starts_reacquire_cooldown() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });
        info.go_to_position_is_chase = true;

        start_chase_reacquire_cooldown_if_chasing(&mut info, 5.0);

        assert_eq!(info.chase_reacquire_timer, 5.0);
    }

    #[test]
    fn non_chase_go_to_does_not_start_reacquire_cooldown() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });

        start_chase_reacquire_cooldown_if_chasing(&mut info, 5.0);

        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn lost_chase_target_only_when_chasing_and_unseen() {
        assert!(lost_chase_target(true, false), "chasing + lost sight → demote");
        assert!(!lost_chase_target(true, true), "still visible → keep chasing");
        assert!(!lost_chase_target(false, false), "not a chase → nothing to demote");
    }

    #[test]
    fn ready_to_acquire_unless_cooldown_or_returning() {
        // Idle/patrol or a chase demoted to a stale last-known spot — re-acquire
        // a visible player regardless of the current go-to.
        assert!(ready_to_acquire_player(false, false));
        // Leash cooldown defers re-acquisition.
        assert!(!ready_to_acquire_player(true, false));
        // An in-progress return-to-spawn is left uninterrupted.
        assert!(!ready_to_acquire_player(false, true));
    }

    #[test]
    fn stall_self_arms_with_full_window() {
        let mut info = actor_info(0.0);
        // Unarmed → arms at the current position with the passed window.
        assert!(!tick_stall(&mut info, &Position::default(), 0.2, 2.0));
        assert_eq!(info.stall_anchor, Some(Position::default()));
        assert_eq!(info.stall_timer, 2.0);
    }

    #[test]
    fn stall_fires_after_no_progress_window() {
        let mut info = actor_info(0.0);
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.1;
        // No displacement and the window (0.1 s) lapses → stalled.
        assert!(tick_stall(&mut info, &Position::default(), 0.2, 1.5));
    }

    #[test]
    fn stall_progress_refills_window() {
        let mut info = actor_info(0.0);
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 0.1;
        let moved = Position { x: 1.0, y: 0.0, z: 0.0 };
        // Displaced well past the threshold → progress → window refilled with
        // the passed (per-state) window, not stalled.
        assert!(!tick_stall(&mut info, &moved, 0.2, 3.0));
        assert_eq!(info.stall_timer, 3.0);
        assert_eq!(info.stall_anchor, Some(moved));
    }

    #[test]
    fn stall_holds_while_window_remains() {
        let mut info = actor_info(0.0);
        info.stall_anchor = Some(Position::default());
        info.stall_timer = 1.0;
        // No displacement but the window isn't spent → not stalled yet.
        assert!(!tick_stall(&mut info, &Position::default(), 0.2, 1.5));
    }

    #[test]
    fn stall_goal_classifies_states() {
        let pos = Position::default();
        let mut info = actor_info(0.0);

        // Idle patrol: intentionally stationary.
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::Exempt);

        // Moving patrol counts.
        info.patrol_intent = ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 2.0,
        };
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::Patrol);

        // Demoted pursuit (stale spot, not a chase).
        info.go_to_position = Some(Position { x: 5.0, y: 0.0, z: 0.0 });
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::DemotedPursuit);

        // Active chase, target out of reach → pressing.
        info.go_to_position_is_chase = true;
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::ActiveChase);

        // Active chase holding under a ledge player: intentionally stationary.
        info.go_to_position = Some(Position { x: 0.3, y: 5.0, z: 0.0 });
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::Exempt);

        // Return takes precedence over everything.
        info.is_returning_to_spawn = true;
        assert_eq!(stall_goal(&info, &pos, 0.5), StallGoal::Return);
    }

    #[test]
    fn stalled_chase_abandons_and_arms_reacquire_cooldown() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });
        info.go_to_position_is_chase = true;
        info.stall_anchor = Some(Position::default());

        abandon_stalled_chase(&mut info, 10.0);

        assert_eq!(info.go_to_position, None);
        assert!(!info.go_to_position_is_chase);
        assert_eq!(info.chase_reacquire_timer, 10.0);
        assert_eq!(info.stall_anchor, None);
    }

    #[test]
    fn stalled_return_clears_state_and_arms_retry_grace() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });
        info.is_returning_to_spawn = true;
        info.return_path.push_back(Position { x: 2.0, y: 0.0, z: 2.0 });

        abandon_stalled_return(&mut info);

        assert_eq!(info.go_to_position, None);
        assert!(!info.is_returning_to_spawn);
        assert!(info.return_path.is_empty());
        assert_eq!(info.return_retry_timer, RETURN_RETRY_SECS);
    }

    #[test]
    fn return_retry_timer_grants_grace_until_elapsed() {
        let mut info = actor_info(0.0);
        info.return_retry_timer = 1.0;

        assert!(tick_return_retry_timer(&mut info, 0.25));
        assert_eq!(info.return_retry_timer, 0.75);
        assert!(!tick_return_retry_timer(&mut info, 0.75));
        assert_eq!(info.return_retry_timer, 0.0);
    }

    #[test]
    fn setting_return_path_marks_actor_as_returning() {
        let mut cells = crate::resources::CellGrid::new(1, 1);
        cells.rows[0][0].has_floor = true;
        let map_config = MapConfig {
            levels: vec![crate::resources::LevelGrid {
                cells,
                edges: crate::resources::EdgeGrid::new(1, 1),
                barrier_edges: crate::resources::EdgeGrid::new(1, 1),
            }],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            cookie_spawn_zones: Vec::new(),
            key_spawn_zones: Vec::new(),
            pressure_plates: Vec::new(),
        };
        let nav_graph = NavGraph::new(map_config, MapGeometry::new(1, 1));
        let zone = crate::resources::ActorSpawnZone {
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: "mine_1".into(),
            count: 1,
        };
        let mut info = actor_info(0.0);

        set_return_path_to_spawn_zone(
            &mut info,
            &Position { x: 0.0, y: 0.0, z: 0.0 },
            &zone,
            &nav_graph,
            (-2.0, -2.0, 2.0, 2.0),
        );

        assert!(info.is_returning_to_spawn);
        assert!(info.go_to_position.is_some());
    }
}
