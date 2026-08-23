use bevy::prelude::Entity;
use rand::{SeedableRng, rngs::StdRng};

use super::{
    controllers::{BeamStarted, decide_beam_actor, decide_contact_actor, decide_contact_beam_actor},
    perception::{PlayerState, update_awareness},
    tick::{BehaviorContext, enter_evade, shake_loose, tick_runtime_state},
};
use crate::{
    actors::{
        ActorInfo, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph},
    },
    config::ServerGameplayConfig,
    map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid, MapConfig},
};
use common::{
    config::GameplayConfig,
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::MapGeometry,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{BarrierKindTable, MapLayout, PlayerId, Position, Wall},
};

struct Fixture {
    graph: NavGraph,
    territories: ActorTerritories,
    collision_world: CollisionWorld,
    gameplay: GameplayConfig,
    server: ServerGameplayConfig,
    geometry: MapGeometry,
}

impl Fixture {
    fn new(kind: &str) -> Self {
        Self::with_world(
            kind,
            CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default()),
        )
    }

    fn with_world(kind: &str, collision_world: CollisionWorld) -> Self {
        Self::with_levels_and_world(kind, 1, collision_world)
    }

    fn with_levels(kind: &str, level_count: usize) -> Self {
        Self::with_levels_and_world(
            kind,
            level_count,
            CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default()),
        )
    }

    fn with_levels_and_world(kind: &str, level_count: usize, collision_world: CollisionWorld) -> Self {
        let cols = 12;
        let rows = 5;
        let levels = (0..level_count)
            .map(|_| {
                let mut cells = CellGrid::new(cols, rows);
                for row in &mut cells.rows {
                    for cell in row {
                        cell.has_floor = true;
                    }
                }
                LevelGrid {
                    cells,
                    edges: EdgeGrid::new(cols, rows),
                    barrier_edges: EdgeGrid::new(cols, rows),
                }
            })
            .collect();
        let map = MapConfig {
            levels,
            actor_spawn_zones: vec![ActorSpawnZone {
                level: 0,
                cols: [1, 2],
                rows: [2, 3],
                kind: kind.to_owned(),
                count: 1,
            }],
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        };
        let geometry = MapGeometry::new(cols, rows);
        let graph = NavGraph::new(map.clone(), geometry);
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let territories = ActorTerritories::new(&graph, &map, &server).expect("test territory should build");
        Self {
            graph,
            territories,
            collision_world,
            gameplay: GameplayConfig::load_default().expect("default gameplay config should load"),
            server,
            geometry,
        }
    }

    fn pos(&self, col: i32, row: i32) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(col) + 2.0,
            y: 0.0,
            z: self.geometry.cell_to_world_z(row) + 2.0,
        }
    }

    fn context(&self, kind: &str, pos: Position) -> BehaviorContext<'_> {
        let actor = self.gameplay.expect_actor(kind);
        BehaviorContext {
            pos,
            actor_physics: actor.physics(),
            actor_eye_height: actor.eye_height(),
            player_physics: self.gameplay.player.physics(),
            nav_graph: &self.graph,
            territory: self.territories.get(0),
            collision_world: &self.collision_world,
            kind_config: self.server.expect_actor(kind),
        }
    }
}

fn info(kind: &str) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, kind.to_owned())
}

fn aware(id: u32, pos: Position, support: CharacterSupport, visible: bool) -> crate::actors::resources::AwarePlayer {
    crate::actors::resources::AwarePlayer {
        id: PlayerId(id),
        pos,
        support,
        visible,
        forget_remaining_secs: 10.0,
        attack_anchor: None,
    }
}

#[test]
fn contact_actor_engages_reachable_ground_player() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("mine");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
    assert_eq!(info.awareness[0].attack_anchor, Some(target));
}

#[test]
fn contact_actor_pursues_reachable_player_outside_home_region() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(10, 2);
    assert!(
        !fixture
            .graph
            .position_in_roam_region(&target, fixture.territories.get(0))
    );
    let mut info = info("mine");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    let route = info.route.as_ref().expect("engagement should install a route");
    assert_eq!(route.destination, target);
    assert_eq!(route.waypoints.len(), 1);
    assert_eq!(route.next(), Some(target));
}

#[test]
fn jumping_target_keeps_its_last_ground_attack_anchor() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let anchor = fixture.pos(4, 2);
    let mut info = info("mine");
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: anchor,
    };
    let mut target = aware(7, Position { y: 2.0, ..anchor }, CharacterSupport::Airborne, true);
    target.attack_anchor = Some(anchor);
    info.awareness.push(target);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Engage { target: PlayerId(7), target_pos } if target_pos == anchor));
}

#[test]
fn ladder_target_makes_contact_actor_evade() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("mine");
    info.awareness.push(aware(
        7,
        Position {
            y: 3.0,
            ..fixture.pos(4, 2)
        },
        CharacterSupport::Ladder,
        true,
    ));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Evade);
}

#[test]
fn reachable_player_has_priority_over_fleeing_from_another_player() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("mine");
    info.awareness.push(aware(
        7,
        Position {
            y: 3.0,
            ..fixture.pos(2, 2)
        },
        CharacterSupport::Ladder,
        true,
    ));
    info.awareness
        .push(aware(8, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(8),
            ..
        }
    ));
}

#[test]
fn ready_zapper_fires_at_visible_player_in_range() {
    let fixture = Fixture::new("zapper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info("zapper");
    info.awareness.push(aware(7, target, CharacterSupport::Ladder, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_beam_actor(&mut info, &fixture.context("zapper", actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Firing { .. }));
    assert_eq!(
        outcome,
        Some(BeamStarted {
            target: PlayerId(7),
            duration_secs: fixture
                .server
                .expect_actor("zapper")
                .combat
                .attack
                .beam()
                .expect("beam config")
                .duration_secs,
        })
    );
}

#[test]
fn zapper_acquires_visible_cross_level_player_in_beam_range() {
    let fixture = Fixture::with_levels("zapper", 2);
    let actor_pos = fixture.pos(1, 2);
    let target = Position {
        y: LEVEL_HEIGHT,
        ..actor_pos
    };
    let kind = fixture.server.expect_actor("zapper");
    let zapper = fixture.gameplay.expect_actor("zapper");
    assert!(
        fixture
            .graph
            .engagement_route(
                &actor_pos,
                &target,
                zapper.physics().collider.width / 2.0,
                zapper.physics().collider.depth / 2.0,
            )
            .is_none()
    );
    assert!(actor_pos.distance_sq(&target) <= kind.combat.attack.beam().expect("beam config").range.powi(2));
    let mut info = info("zapper");
    update_awareness(
        &mut info,
        actor_pos,
        fixture.gameplay.expect_actor("zapper").eye_height(),
        kind.vision_range,
        fixture.server.actor_settings.threat_memory_secs,
        fixture.gameplay.player.physics(),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
        &fixture.collision_world,
    );
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_beam_actor(&mut info, &fixture.context("zapper", actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Firing { .. }));
    assert!(matches!(
        outcome,
        Some(BeamStarted {
            target: PlayerId(7),
            ..
        })
    ));
}

#[test]
fn cooling_zapper_evades_instead_of_approaching() {
    let fixture = Fixture::new("zapper");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("zapper");
    info.beam = BeamState::Cooldown { remaining_secs: 4.0 };
    info.awareness
        .push(aware(7, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_beam_actor(&mut info, &fixture.context("zapper", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Evade);
}

#[test]
fn completed_beam_enters_cooldown_and_evade() {
    let fixture = Fixture::new("zapper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info("zapper");
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        remaining_secs: 0.05,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    tick_runtime_state(
        &mut info,
        actor_pos,
        0.1,
        fixture.server.expect_actor("zapper"),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
    );

    assert_eq!(
        info.beam,
        BeamState::Cooldown {
            remaining_secs: fixture
                .server
                .expect_actor("zapper")
                .combat
                .attack
                .beam()
                .expect("beam config")
                .cooldown_secs,
        }
    );
    assert_eq!(info.decision_timer, 0.0);

    decide_beam_actor(&mut info, &fixture.context("zapper", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Evade);
}

#[test]
fn ready_reaper_fires_and_keeps_its_engagement_route() {
    let fixture = Fixture::new("reaper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("reaper");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context("reaper", actor_pos), &mut rng);

    assert!(matches!(
        info.beam,
        BeamState::Firing {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
    assert_eq!(outcome.map(|started| started.target), Some(PlayerId(7)));
}

#[test]
fn cooling_reaper_keeps_engaging_instead_of_evading() {
    let fixture = Fixture::new("reaper");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("reaper");
    info.beam = BeamState::Cooldown { remaining_secs: 4.0 };
    info.awareness
        .push(aware(7, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context("reaper", actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
    assert_eq!(outcome, None);
}

#[test]
fn firing_reaper_without_reachable_target_holds_facing_beam_target() {
    let fixture = Fixture::new("reaper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info("reaper");
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        remaining_secs: 1.0,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ladder, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_beam_actor(&mut info, &fixture.context("reaper", actor_pos), &mut rng);

    assert_eq!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            target_pos: target,
        }
    );
    assert!(info.route.is_none());
}

#[test]
fn reaper_burst_end_enters_cooldown_without_evading() {
    let fixture = Fixture::new("reaper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("reaper");
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        remaining_secs: 0.05,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    tick_runtime_state(
        &mut info,
        actor_pos,
        0.1,
        fixture.server.expect_actor("reaper"),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
    );
    decide_contact_beam_actor(&mut info, &fixture.context("reaper", actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Cooldown { .. }));
    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
}

#[test]
fn actor_outside_roam_region_routes_home() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(10, 2);
    let mut info = info("mine");
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::ReturnHome);
    assert!(info.route.is_some());
}

#[test]
fn actor_inside_roam_region_chooses_a_roam_route() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("mine");
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("mine", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Roam);
    assert!(info.route.is_some());
}

#[test]
fn occluded_player_keeps_last_seen_state_without_refresh() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(1, 2);
    let player = PlayerState {
        id: PlayerId(7),
        pos: fixture.pos(3, 2),
        support: CharacterSupport::Ground,
    };
    let mut info = info("mine");
    let actor = fixture.gameplay.expect_actor("mine");
    update_awareness(
        &mut info,
        actor_pos,
        actor.eye_height(),
        60.0,
        10.0,
        fixture.gameplay.player.physics(),
        &[player],
        &fixture.collision_world,
    );
    assert_eq!(info.awareness.len(), 1);
    info.awareness[0].forget_remaining_secs = 4.0;

    let wall_x = (actor_pos.x + player.pos.x) / 2.0;
    let blocked_world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: wall_x,
                z1: actor_pos.z - 3.0,
                x2: wall_x,
                z2: actor_pos.z + 3.0,
                width: 0.2,
                level: 0,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let moved_player = PlayerState {
        pos: fixture.pos(4, 2),
        support: CharacterSupport::Ladder,
        ..player
    };
    update_awareness(
        &mut info,
        actor_pos,
        actor.eye_height(),
        60.0,
        10.0,
        fixture.gameplay.player.physics(),
        &[moved_player],
        &blocked_world,
    );

    assert_eq!(info.awareness.len(), 1);
    assert!(!info.awareness[0].visible);
    assert_eq!(info.awareness[0].pos, player.pos);
    assert_eq!(info.awareness[0].support, CharacterSupport::Ground);
    assert_eq!(info.awareness[0].forget_remaining_secs, 4.0);
}

#[test]
fn actor_already_in_stable_cover_holds_position() {
    let open = Fixture::new("mine");
    let actor_pos = open.pos(4, 2);
    let threat = open.pos(1, 2);
    let wall_x = (open.pos(2, 2).x + open.pos(3, 2).x) / 2.0;
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: wall_x,
                z1: actor_pos.z - 3.0,
                x2: wall_x,
                z2: actor_pos.z + 3.0,
                width: 0.2,
                level: 0,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let fixture = Fixture::with_world("mine", world);
    let mut info = info("mine");
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, false));

    enter_evade(&mut info, &fixture.context("mine", actor_pos));

    assert_eq!(info.mode, ActorMode::Evade);
    assert!(info.route.is_none());
}

#[test]
fn evade_route_is_replaced_when_same_cell_threat_exposes_destination() {
    let open = Fixture::new("mine");
    let actor_pos = open.pos(5, 2);
    let destination = open.pos(4, 2);
    let protected_threat = open.pos(1, 2);
    let exposed_threat = Position {
        z: protected_threat.z + 1.4,
        ..protected_threat
    };
    let wall_x = (open.pos(2, 2).x + open.pos(3, 2).x) / 2.0;
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: wall_x,
                z1: destination.z - 0.5,
                x2: wall_x,
                z2: destination.z + 0.5,
                width: 0.2,
                level: 0,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let fixture = Fixture::with_world("mine", world);
    let context = fixture.context("mine", actor_pos);
    assert!(context.stable_cover(&destination, &[protected_threat]));
    assert!(!context.stable_cover(&destination, &[exposed_threat]));
    assert_eq!(
        fixture.graph.node_for_position(&protected_threat),
        fixture.graph.node_for_position(&exposed_threat)
    );

    let mut info = info("mine");
    info.mode = ActorMode::Evade;
    info.route = Some(ActorRoute {
        waypoints: [destination].into(),
        destination,
        destination_node: fixture
            .graph
            .node_for_position(&destination)
            .expect("destination nav node"),
    });
    info.awareness
        .push(aware(7, exposed_threat, CharacterSupport::Ladder, true));

    enter_evade(&mut info, &context);

    assert!(info.route.as_ref().is_none_or(|route| route.destination != destination));
}

#[test]
fn failed_cover_search_waits_before_trying_again() {
    let fixture = Fixture::new("mine");
    let actor_pos = fixture.pos(2, 2);
    let threat = fixture.pos(1, 2);
    let mut waiting = info("mine");
    waiting.mode = ActorMode::Evade;
    waiting.evade_replan_remaining_secs = 0.4;
    waiting.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(&mut waiting, &fixture.context("mine", actor_pos));

    assert!(waiting.route.is_none());

    let mut ready = info("mine");
    ready.mode = ActorMode::Evade;
    ready.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(&mut ready, &fixture.context("mine", actor_pos));

    assert!(ready.route.is_some());
}

fn route_through(waypoints: &[Position], fixture: &Fixture) -> ActorRoute {
    let destination = *waypoints.last().expect("route has a destination");
    ActorRoute {
        waypoints: waypoints.iter().copied().collect(),
        destination,
        destination_node: fixture
            .graph
            .node_for_position(&destination)
            .expect("destination nav node"),
    }
}

fn tick_route(info: &mut ActorInfo, pos: Position, fixture: &Fixture) {
    tick_runtime_state(info, pos, 0.1, fixture.server.expect_actor("mine"), &[]);
}

// The mine-in-the-trench jam: the actor overshot the first waypoint along
// the next leg (a ramp-top transition sits at the actor's own cell centre)
// and must not be sent back for it.
#[test]
fn overshot_waypoint_on_the_next_leg_is_skipped() {
    let fixture = Fixture::new("mine");
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info("mine");
    info.route = Some(route_through(&[first, second], &fixture));
    let overshot = Position {
        x: first.x + 0.9,
        ..first
    };

    tick_route(&mut info, overshot, &fixture);

    assert_eq!(
        info.route.as_ref().map(|route| route.waypoints.front().copied()),
        Some(Some(second))
    );
}

#[test]
fn waypoint_ahead_on_the_next_leg_is_kept() {
    let fixture = Fixture::new("mine");
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info("mine");
    info.route = Some(route_through(&[first, second], &fixture));
    let approaching = Position {
        x: first.x - 0.9,
        ..first
    };

    tick_route(&mut info, approaching, &fixture);

    assert_eq!(
        info.route.as_ref().map(|route| route.waypoints.front().copied()),
        Some(Some(first))
    );
}

#[test]
fn corner_waypoint_is_not_skipped_from_the_side() {
    let fixture = Fixture::new("mine");
    let corner = fixture.pos(2, 2);
    let after = fixture.pos(5, 2);
    let mut info = info("mine");
    info.route = Some(route_through(&[corner, after], &fixture));
    // Approaching the corner along the row axis: beyond it along the next
    // leg by a hair, but a full cell off that leg's line.
    let beside = Position {
        x: corner.x + 0.1,
        z: corner.z + 3.0,
        ..corner
    };

    tick_route(&mut info, beside, &fixture);

    assert_eq!(
        info.route.as_ref().map(|route| route.waypoints.front().copied()),
        Some(Some(corner))
    );
}

#[test]
fn final_waypoint_is_only_dropped_when_reached() {
    let fixture = Fixture::new("mine");
    let only = fixture.pos(2, 2);
    let mut info = info("mine");
    info.route = Some(route_through(&[only], &fixture));
    let past = Position {
        x: only.x + 0.9,
        ..only
    };

    tick_route(&mut info, past, &fixture);

    assert!(info.route.is_some());
}

#[test]
fn stalled_actor_hops_to_a_random_neighbor_before_rethinking() {
    let fixture = Fixture::new("mine");
    let mut info = info("mine");
    let pos = fixture.pos(5, 2);
    info.set_route(Some(route_through(&[fixture.pos(9, 2)], &fixture)));

    let mut stalled = false;
    for _ in 0..20 {
        stalled = tick_runtime_state(&mut info, pos, 0.1, fixture.server.expect_actor("mine"), &[]);
        if stalled {
            break;
        }
    }
    assert!(stalled, "pinned actor must trip the watchdog");

    let mut rng = StdRng::seed_from_u64(1);
    shake_loose(&mut info, &fixture.context("mine", pos), &mut rng);

    let route = info.route.as_ref().expect("shake installs a hop route");
    assert_eq!(route.waypoints.len(), 1, "one-leg hop");
    let hop_distance = pos.horizontal_distance_sq(&route.destination).sqrt();
    assert!(
        hop_distance > 0.1 && hop_distance < GRID_CELL_SIZE * 1.6,
        "hop lands in a neighboring cell, got {hop_distance}"
    );
    assert!(info.decision_timer > 0.0, "controller deferred during the hop");
}
