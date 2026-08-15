use std::collections::VecDeque;

use bevy::prelude::Entity;

use super::patrol::fresh_patrol_goal;
use super::tick::{
    BehaviorInputs, CHASE_GIVEUP_NO_PROGRESS_SECS, PATROL_GIVEUP_NO_PROGRESS_SECS, PATROL_LEDGE_ESCAPE_SECS,
    PURSUIT_GIVEUP_NO_PROGRESS_SECS, RETURN_GIVEUP_NO_PROGRESS_SECS, RETURN_RETRY_SECS, patrolling_off_zone_level,
    stall_window, tick_actor_behavior, tick_chase_reacquire_timer, tick_return_retry_timer, tick_stall,
};
use crate::{
    actors::navigation::NavGraph,
    actors::{ActorGoal, ActorInfo},
    config::ServerGameplayConfig,
    map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid, MapConfig},
};
use common::{
    constants::LEVEL_HEIGHT,
    map::MapGeometry,
    protocol::{ActorMoveIntent, PlayerId, Position},
};

fn actor_info(goal: ActorGoal) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, "mine".into(), goal)
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
        placed_items: Vec::new(),
        pressure_plates: Vec::new(),
    };
    NavGraph::new(map_config, MapGeometry::new(1, 1))
}

fn zone() -> ActorSpawnZone {
    ActorSpawnZone {
        level: 0,
        cols: [0, 1],
        rows: [0, 1],
        kind: "mine".into(),
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
            visible_player: visible_player.map(|target| (PlayerId(7), target)),
            fire_target_position: None,
            zone: &self.zone,
            zone_bounds: (-2.0, -2.0, 2.0, 2.0),
            nav_graph: &self.nav,
            patrol_speed: 2.0,
            kind_config: self.config.validated_actor("mine"),
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
    assert_eq!(stall_window(&fresh_patrol_goal(), &pos, 0.5, None), None);
    // Moving patrol counts.
    assert_eq!(
        stall_window(&moving_patrol(), &pos, 0.5, None),
        Some(PATROL_GIVEUP_NO_PROGRESS_SECS)
    );
    // Pressing chase.
    let chase = ActorGoal::Chase {
        target: Position { x: 5.0, y: 0.0, z: 0.0 },
    };
    assert_eq!(
        stall_window(&chase, &pos, 0.5, None),
        Some(CHASE_GIVEUP_NO_PROGRESS_SECS)
    );
    // Holding under a ledge player: intentionally stationary.
    let holding = ActorGoal::Chase {
        target: Position { x: 0.3, y: 5.0, z: 0.0 },
    };
    assert_eq!(stall_window(&holding, &pos, 0.5, None), None);
    let pursuit = ActorGoal::Pursuit {
        last_seen: Position { x: 5.0, y: 0.0, z: 0.0 },
    };
    assert_eq!(
        stall_window(&pursuit, &pos, 0.5, None),
        Some(PURSUIT_GIVEUP_NO_PROGRESS_SECS)
    );
    let returning = ActorGoal::Return {
        next: Position { x: 5.0, y: 0.0, z: 0.0 },
        path: VecDeque::new(),
    };
    assert_eq!(
        stall_window(&returning, &pos, 0.5, None),
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

    // mine has idle_probability 0, so the same-tick re-roll must yield
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
            .validated_actor("mine")
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

// ---- laser (zapper) transitions ----------------------------------------

fn zapper_info(goal: ActorGoal) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, "zapper".into(), goal)
}

impl Fixture {
    // Like `inputs`, but for the laser kind. Fields (e.g.
    // `fire_target_position`, `beyond_leash`) are pub — override after
    // construction where a test needs them.
    fn zapper_inputs(&self, pos: Position, visible_player: Option<Position>) -> BehaviorInputs<'_> {
        BehaviorInputs {
            pos,
            delta: 0.1,
            beyond_leash: false,
            visible_player: visible_player.map(|target| (PlayerId(7), target)),
            fire_target_position: None,
            zone: &self.zone,
            zone_bounds: (-2.0, -2.0, 2.0, 2.0),
            nav_graph: &self.nav,
            patrol_speed: 2.0,
            kind_config: self.config.validated_actor("zapper"),
        }
    }
}

#[test]
fn patrol_acquires_approach_for_laser_kind() {
    let fixture = Fixture::new();
    let mut info = zapper_info(moving_patrol());
    // Visible but beyond fire range (34 m): must approach, never Chase.
    let target = Position {
        x: 36.0,
        y: 0.0,
        z: 0.0,
    };

    tick(&mut info, &fixture.zapper_inputs(Position::default(), Some(target)));

    assert_eq!(
        info.goal,
        ActorGoal::Approach {
            target: PlayerId(7),
            target_pos: target
        }
    );
}

#[test]
fn approach_enters_fire_within_range_when_cooldown_ready() {
    let fixture = Fixture::new();
    let target = Position {
        x: 12.0,
        y: 0.0,
        z: 0.0,
    };
    let mut info = zapper_info(ActorGoal::Approach {
        target: PlayerId(7),
        target_pos: target,
    });

    tick(&mut info, &fixture.zapper_inputs(Position::default(), Some(target)));

    assert_eq!(
        info.goal,
        ActorGoal::Fire {
            target: PlayerId(7),
            target_pos: target,
            remaining_secs: 2.0,
        }
    );
}

#[test]
fn approach_stays_while_cooldown_running() {
    let fixture = Fixture::new();
    let target = Position {
        x: 12.0,
        y: 0.0,
        z: 0.0,
    };
    let mut info = zapper_info(ActorGoal::Approach {
        target: PlayerId(7),
        target_pos: target,
    });
    info.fire_cooldown_timer = 5.0;

    tick(&mut info, &fixture.zapper_inputs(Position::default(), Some(target)));

    assert_eq!(
        info.goal,
        ActorGoal::Approach {
            target: PlayerId(7),
            target_pos: target
        }
    );
}

#[test]
fn fire_refreshes_target_position_each_tick() {
    let fixture = Fixture::new();
    let mut info = zapper_info(ActorGoal::Fire {
        target: PlayerId(7),
        target_pos: Position { x: 5.0, y: 0.0, z: 0.0 },
        remaining_secs: 1.0,
    });
    let live = Position { x: 6.0, y: 0.0, z: 1.0 };
    let mut inputs = fixture.zapper_inputs(Position::default(), None);
    inputs.fire_target_position = Some(live);

    tick(&mut info, &inputs);

    let ActorGoal::Fire {
        target_pos,
        remaining_secs,
        ..
    } = info.goal
    else {
        panic!("expected the burst to continue, got {:?}", info.goal);
    };
    assert_eq!(target_pos, live);
    assert!((remaining_secs - 0.9).abs() < 1e-4);
}

#[test]
fn fire_expires_into_flee_and_arms_cooldown() {
    let fixture = Fixture::new();
    let last_pos = Position { x: 5.0, y: 0.0, z: 0.0 };
    let mut info = zapper_info(ActorGoal::Fire {
        target: PlayerId(7),
        target_pos: last_pos,
        remaining_secs: 0.05,
    });
    let mut inputs = fixture.zapper_inputs(Position::default(), None);
    inputs.fire_target_position = Some(last_pos);

    tick(&mut info, &inputs);

    assert_eq!(info.goal, ActorGoal::Flee { threat: last_pos });
    assert_eq!(info.fire_cooldown_timer, 5.0);
    // The same-tick watchdog re-arms a fresh window for the new flee.
    assert_eq!(info.stall_anchor, Some(Position::default()));
    assert_eq!(info.stall_timer, CHASE_GIVEUP_NO_PROGRESS_SECS);
}

#[test]
fn fire_ends_early_when_target_gone() {
    let fixture = Fixture::new();
    let last_pos = Position { x: 5.0, y: 0.0, z: 0.0 };
    let mut info = zapper_info(ActorGoal::Fire {
        target: PlayerId(7),
        target_pos: last_pos,
        remaining_secs: 0.5,
    });

    // `fire_target_position: None` = target died or logged off.
    tick(&mut info, &fixture.zapper_inputs(Position::default(), None));

    assert_eq!(info.goal, ActorGoal::Flee { threat: last_pos });
    assert_eq!(info.fire_cooldown_timer, 5.0);
}

#[test]
fn fire_and_approach_hold_are_stall_exempt() {
    let fixture = Fixture::new();
    let fire = fixture
        .config
        .validated_actor("zapper")
        .fire
        .as_ref()
        .expect("zapper fire config missing from server gameplay config");
    let pos = Position::default();

    let firing = ActorGoal::Fire {
        target: PlayerId(7),
        target_pos: Position { x: 5.0, y: 0.0, z: 0.0 },
        remaining_secs: 0.5,
    };
    assert_eq!(stall_window(&firing, &pos, 0.5, Some(fire)), None);

    // Holding at standoff (target within 20 m): intentionally stationary.
    let holding = ActorGoal::Approach {
        target: PlayerId(7),
        target_pos: Position {
            x: 15.0,
            y: 0.0,
            z: 0.0,
        },
    };
    assert_eq!(stall_window(&holding, &pos, 0.5, Some(fire)), None);

    // Pressing from beyond standoff: chase window applies.
    let pressing = ActorGoal::Approach {
        target: PlayerId(7),
        target_pos: Position {
            x: 30.0,
            y: 0.0,
            z: 0.0,
        },
    };
    assert_eq!(
        stall_window(&pressing, &pos, 0.5, Some(fire)),
        Some(CHASE_GIVEUP_NO_PROGRESS_SECS)
    );
}

#[test]
fn flee_ends_into_patrol_when_cooldown_expires() {
    let fixture = Fixture::new();
    let mut info = zapper_info(ActorGoal::Flee {
        threat: Position { x: 5.0, y: 0.0, z: 0.0 },
    });
    info.fire_cooldown_timer = 0.05;

    tick(&mut info, &fixture.zapper_inputs(Position::default(), None));

    assert!(matches!(info.goal, ActorGoal::Patrol { .. }));
}

#[test]
fn flee_tracks_visible_threat_without_reengaging() {
    let fixture = Fixture::new();
    let mut info = zapper_info(ActorGoal::Flee {
        threat: Position { x: 5.0, y: 0.0, z: 0.0 },
    });
    info.fire_cooldown_timer = 5.0;
    let live = Position { x: 3.0, y: 0.0, z: 3.0 };

    tick(&mut info, &fixture.zapper_inputs(Position::default(), Some(live)));

    assert_eq!(info.goal, ActorGoal::Flee { threat: live });
}

#[test]
fn flee_beyond_leash_preempts_to_return() {
    let fixture = Fixture::new();
    let mut info = zapper_info(ActorGoal::Flee {
        threat: Position { x: 5.0, y: 0.0, z: 0.0 },
    });
    info.fire_cooldown_timer = 5.0;
    let pos = Position {
        x: 40.0,
        y: 0.0,
        z: 0.0,
    };
    let mut inputs = fixture.zapper_inputs(pos, None);
    inputs.beyond_leash = true;

    tick(&mut info, &inputs);

    assert!(matches!(info.goal, ActorGoal::Return { .. }));
    // The re-fire lockout survives the preemption — it lives on `ActorInfo`,
    // not in the goal.
    assert!(info.fire_cooldown_timer > 0.0);
}

#[test]
fn approach_demotes_to_pursuit_on_lost_sight() {
    let fixture = Fixture::new();
    let last_seen = Position { x: 5.0, y: 0.0, z: 5.0 };
    let mut info = zapper_info(ActorGoal::Approach {
        target: PlayerId(7),
        target_pos: last_seen,
    });

    tick(&mut info, &fixture.zapper_inputs(Position::default(), None));

    assert_eq!(info.goal, ActorGoal::Pursuit { last_seen });
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
