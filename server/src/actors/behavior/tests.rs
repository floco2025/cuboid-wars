use bevy::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{
    controllers::{
        decide_beam_actor, decide_contact_actor, decide_contact_beam_actor, decide_stationary_actor, retarget_beam,
    },
    perception::{PlayerState, update_awareness},
    tick::{
        BehaviorContext, EVADE_REPLAN_INTERVAL_SECS, enter_evade, keep_or_install_engagement_route, shake_loose,
        tick_runtime_state,
    },
};
use crate::{
    actors::ActorMap,
    actors::{
        ActorInfo, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph, NavGraphs, NavWaypoint, WaypointKind},
    },
    combat::{PendingExplosions, actors_beam_damage_system},
    config::{
        ActorAttackConfig, ActorBeamAttackConfig, ContactAttackConfig, ContactBeamAttackConfig, ServerGameplayConfig,
    },
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    network::ServerToClient,
    players::{Invincibility, PlayerInfo, PlayerMap},
    test_geometry::{CELL, LEVEL_HEIGHT, WALL_HEIGHT, geometry},
};
use common::{
    config::GameplayConfig,
    constants::TICK_SECS,
    map::{CarrierPose, Carriers, MapGeometry},
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        ActorBeam, ActorId, ActorMarker, Barrier, BarrierKindId, BarrierKindTable, Carrier, CarrierId, Health,
        MapItems, MapLayout, PlateState, PlayerId, PlayerMarker, Position, ServerMessage, ServerTick, Wall,
    },
};

// A 12x5 all-floor grid with one zone at (1, 2). With a carrier, the zone's
// grid is nested on a static carrier resting at `rest`, so `pos` is in the
// carrier's frame and the world is `pose()` away.
struct Fixture {
    graphs: NavGraphs,
    territories: ActorTerritories,
    collision_world: CollisionWorld,
    gameplay: GameplayConfig,
    server: ServerGameplayConfig,
    geometry: MapGeometry,
    carrier: CarrierId,
    carriers: Carriers,
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
        Self::build(kind, level_count, collision_world, None)
    }

    fn with_carrier(kind: &str, rest: Position) -> Self {
        Self::build(
            kind,
            1,
            CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default()),
            Some(rest),
        )
    }

    fn build(kind: &str, level_count: usize, collision_world: CollisionWorld, rest: Option<Position>) -> Self {
        let cols = 12;
        let rows = 5;
        let levels = |count: usize| -> Vec<LevelGrid> {
            (0..count)
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
                .collect()
        };
        let geometry = geometry(cols, rows);
        let carrier = if rest.is_some() { CarrierId(1) } else { CarrierId::WORLD };
        let mut map = MapConfig {
            actor_spawn_zones: vec![ActorSpawnZone {
                carrier,
                level: 0,
                cols: [1, 2],
                rows: [2, 3],
                kind: kind.to_owned(),
                count: 1,
            }],
            ..MapConfig::for_grid(levels(level_count), geometry)
        };
        let mut layout = MapLayout::default();
        if let Some(rest) = rest {
            map.grids.push(CarrierGrid::new(carrier, geometry, levels(1)));
            layout.carriers.push(Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: rest,
                to: rest,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
            });
        }
        let carriers = Carriers::from_layout(&layout);
        let graphs = NavGraphs::new(&map);
        let mut server = ServerGameplayConfig::load_default().expect("default server gameplay config invalid");
        if kind == "hybrid" {
            let mut actor = server.expect_actor("bruiser").clone();
            actor.attack = ActorAttackConfig::ContactBeam(ContactBeamAttackConfig {
                contact: ContactAttackConfig { trigger_gap: 0.8 },
                beam: ActorBeamAttackConfig {
                    range: 25.0,
                    duration_secs: 2.0,
                    cooldown_secs: 5.0,
                },
            });
            server.actors.kinds.insert(kind.to_owned(), actor);
            let damage = *server.combat.damage.expect_actor("zapper");
            server.combat.damage.actors.insert(kind.to_owned(), damage);
        }
        let territories = ActorTerritories::new(&graphs, &map, &server).expect("test territory should build");
        let gameplay = server.gameplay_config();
        Self {
            graphs,
            territories,
            collision_world,
            gameplay,
            server,
            geometry,
            carrier,
            carriers,
        }
    }

    fn graph(&self) -> &NavGraph {
        self.graphs.get(self.carrier)
    }

    fn pose(&self) -> CarrierPose {
        self.carriers.pose(self.carrier)
    }

    // Off-centre inside the cell, in the zone's grid frame.
    fn pos(&self, col: i32, row: i32) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(col) + 2.0,
            y: 0.0,
            z: self.geometry.cell_to_world_z(row) + 2.0,
        }
    }

    fn context(&self, kind: &str, pos: Position) -> BehaviorContext<'_> {
        let actor = self.gameplay.expect_actor(kind);
        let pose = self.pose();
        BehaviorContext {
            tick: 0,
            pos,
            world_pos: pose.transform_position(&pos),
            pose,
            actor_physics: actor.physics(),
            actor_eye_height: actor.eye_height(),
            player_physics: self.gameplay.player.physics(),
            nav_graph: self.graph(),
            territory: self.territories.get(0),
            collision_world: &self.collision_world,
            open_barriers: &[],
            kind_config: self.server.expect_actor(kind),
            players_armed: true,
        }
    }
}

fn info(kind: &str) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, kind.to_owned(), CarrierId::WORLD)
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
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("scuttler");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

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
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(10, 2);
    assert!(
        !fixture
            .graph()
            .position_in_roam_region(&target, fixture.territories.get(0))
    );
    let mut info = info("scuttler");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

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
    assert_eq!(route.next().map(|point| point.position), Some(target));
}

#[test]
fn jumping_target_keeps_its_last_ground_attack_anchor() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let anchor = fixture.pos(4, 2);
    let mut info = info("scuttler");
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: anchor,
    };
    let mut target = aware(7, Position { y: 2.0, ..anchor }, CharacterSupport::Airborne, true);
    target.attack_anchor = Some(anchor);
    info.awareness.push(target);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Engage { target: PlayerId(7), target_pos } if target_pos == anchor));
}

#[test]
fn ladder_target_makes_contact_actor_evade() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("scuttler");
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

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
}

#[test]
fn reachable_player_has_priority_over_fleeing_from_another_player() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("scuttler");
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

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

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
        Some(ActorBeam {
            target: PlayerId(7),
            started_tick: 0,
            remaining_secs: fixture
                .server
                .expect_actor("zapper")
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
            .graph()
            .engagement_route(
                &[],
                &actor_pos,
                &target,
                zapper.physics().movement_collider.radius(),
                zapper.physics().movement_collider.radius(),
            )
            .is_none()
    );
    assert!(actor_pos.distance_sq(&target) <= kind.attack.beam().expect("beam config").range.powi(2));
    let mut info = info("zapper");
    update_awareness(
        &mut info,
        actor_pos,
        fixture.gameplay.expect_actor("zapper").eye_height(),
        kind.vision_range,
        fixture.server.actors.settings.threat_memory_secs,
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
        Some(ActorBeam {
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

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
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
        started_tick: 0,
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
                .attack
                .beam()
                .expect("beam config")
                .cooldown_secs,
        }
    );
    assert_eq!(info.decision_timer, 0.0);

    decide_beam_actor(&mut info, &fixture.context("zapper", actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
}

#[test]
fn ready_hybrid_fires_and_keeps_its_engagement_route() {
    let fixture = Fixture::new("hybrid");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("hybrid");
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context("hybrid", actor_pos), &mut rng);

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
fn cooling_hybrid_keeps_engaging_instead_of_evading() {
    let fixture = Fixture::new("hybrid");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("hybrid");
    info.beam = BeamState::Cooldown { remaining_secs: 4.0 };
    info.awareness
        .push(aware(7, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context("hybrid", actor_pos), &mut rng);

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
fn firing_hybrid_without_reachable_target_holds_facing_beam_target() {
    let fixture = Fixture::new("hybrid");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info("hybrid");
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 1.0,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ladder, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_beam_actor(&mut info, &fixture.context("hybrid", actor_pos), &mut rng);

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
fn hybrid_burst_end_enters_cooldown_without_evading() {
    let fixture = Fixture::new("hybrid");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info("hybrid");
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 0.05,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    tick_runtime_state(
        &mut info,
        actor_pos,
        0.1,
        fixture.server.expect_actor("hybrid"),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
    );
    decide_contact_beam_actor(&mut info, &fixture.context("hybrid", actor_pos), &mut rng);

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
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(10, 2);
    let mut info = info("scuttler");
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::ReturnHome);
    assert!(info.route.is_some());
}

#[test]
fn actor_inside_roam_region_chooses_a_roam_route() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("scuttler");
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Roam);
    assert!(info.route.is_some());
}

#[test]
fn occluded_player_keeps_last_seen_state_without_refresh() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let player = PlayerState {
        id: PlayerId(7),
        pos: fixture.pos(3, 2),
        support: CharacterSupport::Ground,
    };
    let mut info = info("scuttler");
    let actor = fixture.gameplay.expect_actor("scuttler");
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
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
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
    let open = Fixture::new("scuttler");
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
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let fixture = Fixture::with_world("scuttler", world);
    let mut info = info("scuttler");
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, false));

    enter_evade(
        &mut info,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
    assert!(info.route.is_none());
}

#[test]
fn evade_route_is_replaced_when_same_cell_threat_exposes_destination() {
    let open = Fixture::new("scuttler");
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
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let fixture = Fixture::with_world("scuttler", world);
    let context = fixture.context("scuttler", actor_pos);
    assert!(context.stable_cover(&destination, &[protected_threat]));
    assert!(!context.stable_cover(&destination, &[exposed_threat]));
    assert_eq!(
        fixture.graph().node_for_position(&protected_threat),
        fixture.graph().node_for_position(&exposed_threat)
    );

    let mut info = info("scuttler");
    info.mode = ActorMode::Evade { fleeing: false };
    info.route = Some(ActorRoute {
        waypoints: [destination].map(NavWaypoint::walk).into(),
        destination,
        destination_node: fixture
            .graph()
            .node_for_position(&destination)
            .expect("destination nav node"),
    });
    info.awareness
        .push(aware(7, exposed_threat, CharacterSupport::Ladder, true));

    enter_evade(&mut info, &context, &mut StdRng::seed_from_u64(1));

    assert!(info.route.as_ref().is_none_or(|route| route.destination != destination));
}

#[test]
fn failed_cover_search_waits_before_trying_again() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(2, 2);
    let threat = fixture.pos(1, 2);
    let mut waiting = info("scuttler");
    waiting.mode = ActorMode::Evade { fleeing: false };
    waiting.evade_replan_remaining_secs = 0.4;
    waiting.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut waiting,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(waiting.route.is_none());

    let mut ready = info("scuttler");
    ready.mode = ActorMode::Evade { fleeing: false };
    ready.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut ready,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(ready.route.is_some());
}

#[test]
fn no_cover_in_reach_sends_the_actor_fleeing_from_the_threat() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(4, 2);
    let threat = fixture.pos(1, 2);
    let mut info = info("scuttler");
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut info,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert_eq!(info.mode, ActorMode::Evade { fleeing: true });
    let route = info.route.as_ref().expect("a flight leg");
    assert!(
        route.destination.horizontal_distance_sq(&threat) > actor_pos.horizontal_distance_sq(&threat),
        "ran toward the threat: {:?}",
        route.destination
    );
}

#[test]
fn a_flight_leg_is_kept_until_it_ends() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(4, 2);
    let threat = fixture.pos(1, 2);
    let leg = route_through(&[fixture.pos(5, 2), fixture.pos(6, 2)], &fixture);
    let mut info = info("scuttler");
    info.mode = ActorMode::Evade { fleeing: true };
    info.route = Some(leg.clone());
    info.evade_replan_remaining_secs = 0.0;
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut info,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert_eq!(info.route, Some(leg));

    info.route = None;
    info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;

    enter_evade(
        &mut info,
        &fixture.context("scuttler", actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(info.route.is_some(), "no new leg on arrival");
}

#[test]
fn unarmed_players_are_not_evaded() {
    let fixture = Fixture::new("scuttler");
    let actor_pos = fixture.pos(1, 2);
    let mut info = info("scuttler");
    info.awareness.push(aware(
        7,
        Position {
            y: 3.0,
            ..fixture.pos(4, 2)
        },
        CharacterSupport::Ladder,
        true,
    ));
    let context = BehaviorContext {
        players_armed: false,
        ..fixture.context("scuttler", actor_pos)
    };

    decide_contact_actor(&mut info, &context, &mut StdRng::seed_from_u64(1));

    assert!(!matches!(info.mode, ActorMode::Evade { .. }), "evaded: {:?}", info.mode);
}

fn route_through(waypoints: &[Position], fixture: &Fixture) -> ActorRoute {
    let destination = *waypoints.last().expect("route has a destination");
    ActorRoute {
        waypoints: waypoints.iter().copied().map(NavWaypoint::walk).collect(),
        destination,
        destination_node: fixture
            .graph()
            .node_for_position(&destination)
            .expect("destination nav node"),
    }
}

fn tick_route(info: &mut ActorInfo, pos: Position, fixture: &Fixture) {
    tick_runtime_state(info, pos, 0.1, fixture.server.expect_actor("scuttler"), &[]);
}

// The scuttler-in-the-trench jam: the actor overshot the first waypoint along
// the next leg (a ramp-top transition sits at the actor's own cell centre)
// and must not be sent back for it.
#[test]
fn overshot_waypoint_on_the_next_leg_is_skipped() {
    let fixture = Fixture::new("scuttler");
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info("scuttler");
    info.route = Some(route_through(&[first, second], &fixture));
    let overshot = Position {
        x: first.x + 0.9,
        ..first
    };

    tick_route(&mut info, overshot, &fixture);

    assert_eq!(
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(second))
    );
}

#[test]
fn waypoint_ahead_on_the_next_leg_is_kept() {
    let fixture = Fixture::new("scuttler");
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info("scuttler");
    info.route = Some(route_through(&[first, second], &fixture));
    let approaching = Position {
        x: first.x - 0.9,
        ..first
    };

    tick_route(&mut info, approaching, &fixture);

    assert_eq!(
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(first))
    );
}

#[test]
fn corner_waypoint_is_not_skipped_from_the_side() {
    let fixture = Fixture::new("scuttler");
    let corner = fixture.pos(2, 2);
    let after = fixture.pos(5, 2);
    let mut info = info("scuttler");
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
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(corner))
    );
}

#[test]
fn final_waypoint_is_only_dropped_when_reached() {
    let fixture = Fixture::new("scuttler");
    let only = fixture.pos(2, 2);
    let mut info = info("scuttler");
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
    let fixture = Fixture::new("scuttler");
    let mut info = info("scuttler");
    let pos = fixture.pos(5, 2);
    info.set_route(Some(route_through(&[fixture.pos(9, 2)], &fixture)));

    let mut stalled = false;
    for _ in 0..20 {
        stalled = tick_runtime_state(&mut info, pos, 0.1, fixture.server.expect_actor("scuttler"), &[]);
        if stalled {
            break;
        }
    }
    assert!(stalled, "pinned actor must trip the watchdog");

    let mut rng = StdRng::seed_from_u64(1);
    shake_loose(&mut info, &fixture.context("scuttler", pos), &mut rng);

    let route = info.route.as_ref().expect("shake installs a hop route");
    assert_eq!(route.waypoints.len(), 1, "one-leg hop");
    let hop_distance = pos.horizontal_distance_sq(&route.destination).sqrt();
    assert!(
        hop_distance > 0.1 && hop_distance < CELL * 1.6,
        "hop lands in a neighboring cell, got {hop_distance}"
    );
    assert!(info.decision_timer > 0.0, "controller deferred during the hop");
}

// === Carriers ===

fn carrier_rest() -> Position {
    Position {
        x: 40.0,
        y: LEVEL_HEIGHT,
        z: -30.0,
    }
}

#[test]
fn carried_actor_roams_in_its_carriers_frame() {
    let fixture = Fixture::with_carrier("scuttler", carrier_rest());
    let mut info = info("scuttler");
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context("scuttler", fixture.pos(1, 2)), &mut rng);

    assert_eq!(info.mode, ActorMode::Roam);
    let route = info.route.expect("a roam route");
    for waypoint in &route.waypoints {
        assert!(
            fixture.graph().contains(&waypoint.position),
            "{waypoint:?} is off the carrier's grid"
        );
        assert_eq!(waypoint.position.y, 0.0);
    }
}

#[test]
fn player_off_the_carrier_is_unreachable() {
    let fixture = Fixture::with_carrier("scuttler", carrier_rest());
    let mut info = info("scuttler");
    let context = fixture.context("scuttler", fixture.pos(1, 2));
    let beside = Position {
        x: carrier_rest().x + fixture.geometry.width() / 2.0 + CELL,
        ..carrier_rest()
    };
    let underneath = Position {
        y: carrier_rest().y - LEVEL_HEIGHT,
        ..context.world_pos
    };

    assert!(!keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        beside
    ));
    assert!(!keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        underneath
    ));
    assert!(info.route.is_none());
}

#[test]
fn player_aboard_the_carrier_is_engaged_along_a_carrier_local_route() {
    let fixture = Fixture::with_carrier("scuttler", carrier_rest());
    let mut info = info("scuttler");
    let context = fixture.context("scuttler", fixture.pos(1, 2));
    let target_local = fixture.pos(8, 2);
    let target_world = fixture.pose().transform_position(&target_local);

    assert!(keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        target_world
    ));

    let route = info.route.as_ref().expect("an engagement route");
    assert!(
        route.destination.distance_sq(&target_local) < 1e-6,
        "{:?}",
        route.destination
    );
    assert!(
        route
            .waypoints
            .iter()
            .all(|waypoint| fixture.graph().contains(&waypoint.position))
    );
    assert_eq!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(1),
            target_pos: target_world
        }
    );
}

#[test]
fn cover_is_judged_at_the_candidates_world_position() {
    let fixture = Fixture::with_carrier("scuttler", carrier_rest());
    let context = fixture.context("scuttler", fixture.pos(1, 2));
    let candidate = fixture.pos(8, 2);
    // A threat standing on the candidate in the world; in an open world
    // nothing is ever cover, so only the too-close test can decide.
    let threat_on_it = fixture.pose().transform_position(&candidate);
    let threat_far = Position {
        x: threat_on_it.x + 100.0,
        ..threat_on_it
    };

    assert!(!context.stable_cover(&candidate, &[threat_on_it]));
    assert!(!context.stable_cover(&candidate, &[threat_far]));
}

#[test]
fn climbing_progress_uses_height_and_does_not_skip_to_the_exit() {
    let fixture = Fixture::new("scuttler");
    let mut info = info("scuttler");
    let bottom = fixture.pos(2, 2);
    let top = Position {
        y: LEVEL_HEIGHT,
        ..bottom
    };
    let exit = Position { x: top.x + CELL, ..top };
    let mut route = route_through(&[top, exit], &fixture);
    route.waypoints[0].kind = WaypointKind::Climb {
        normal_x: 0.0,
        normal_z: -1.0,
        ascending: true,
    };
    route.waypoints[1].kind = WaypointKind::Exit;
    info.set_route(Some(route));
    for tick in 0..30 {
        let pos = Position {
            y: tick as f32 * 0.1,
            ..bottom
        };
        assert!(!tick_runtime_state(
            &mut info,
            pos,
            0.1,
            fixture.server.expect_actor("scuttler"),
            &[]
        ));
        assert_eq!(info.route.as_ref().expect("ladder route missing").waypoints.len(), 2);
    }
    tick_route(&mut info, top, &fixture);
    assert_eq!(info.route.as_ref().expect("ladder route missing").waypoints.len(), 1);
    tick_route(&mut info, Position { y: 0.0, ..exit }, &fixture);
    assert!(
        info.route.is_some(),
        "horizontal proximity cannot finish a ladder exit at another height"
    );
    tick_route(&mut info, exit, &fixture);
    assert!(info.route.is_none());
}

#[test]
fn reached_ladder_target_remains_a_hold_instead_of_becoming_idle() {
    let fixture = Fixture::new("scuttler");
    let mut info = info("scuttler");
    let target = Position {
        y: 2.0,
        ..fixture.pos(2, 2)
    };
    let mut route = route_through(&[target], &fixture);
    route.waypoints[0].kind = WaypointKind::Climb {
        normal_x: 0.0,
        normal_z: -1.0,
        ascending: true,
    };
    info.set_route(Some(route));
    for _ in 0..30 {
        assert!(!tick_runtime_state(
            &mut info,
            target,
            0.1,
            fixture.server.expect_actor("scuttler"),
            &[]
        ));
    }
    assert!(info.route.is_some());
}

#[test]
fn zapper_sees_a_player_through_a_barrier_but_waits_for_a_clear_attack() {
    let mut fixture = Fixture::new("zapper");
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let kind = BarrierKindId(0);
    let x = (actor_pos.x + target.x) / 2.0;
    let layout = MapLayout {
        barriers: vec![Barrier {
            x1: x,
            x2: x,
            z1: actor_pos.z - 4.0,
            z2: actor_pos.z + 4.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let kinds = BarrierKindTable::from_ids(vec!["shield".into()]).expect("barrier catalog rejected");
    fixture.collision_world = CollisionWorld::from_map_layout(&layout, &kinds);
    let mut info = info("zapper");
    update_awareness(
        &mut info,
        actor_pos,
        fixture.gameplay.expect_actor("zapper").eye_height(),
        60.0,
        1.0,
        fixture.gameplay.player.physics(),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
        &fixture.collision_world,
    );
    assert!(info.awareness[0].visible);
    let mut rng = StdRng::seed_from_u64(1);
    let mut context = fixture.context("zapper", actor_pos);
    assert!(decide_beam_actor(&mut info, &context, &mut rng).is_none());
    assert!(matches!(info.beam, BeamState::Ready));
    let opened = [kind];
    context.open_barriers = &opened;
    assert!(decide_beam_actor(&mut info, &context, &mut rng).is_some());
}

#[test]
fn turret_keeps_exposed_target_and_retargets_without_cooldown() {
    let fixture = Fixture::with_levels("turret", 2);
    let actor_pos = fixture.pos(1, 2);
    let mut state = info("turret");
    let mut above = fixture.pos(3, 2);
    above.y = LEVEL_HEIGHT;
    state.awareness = vec![
        aware(7, above, CharacterSupport::Ground, true),
        aware(8, fixture.pos(2, 2), CharacterSupport::Ground, true),
    ];
    state.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 14.0,
    };
    let context = fixture.context("turret", actor_pos);
    for _ in 0..120 {
        tick_runtime_state(&mut state, actor_pos, 1.0 / 30.0, context.kind_config, &[]);
        retarget_beam(&mut state, &context);
        assert_eq!(state.beam.target(), Some(PlayerId(7)));
        assert!(state.route.is_none());
    }
    let before = state.beam.snapshot().expect("burst missing");
    state.awareness[0].visible = false;
    retarget_beam(&mut state, &context);
    let after = state.beam.snapshot().expect("burst ended during retarget");
    assert_eq!(after.target, PlayerId(8));
    assert_eq!(after.started_tick, before.started_tick);
    assert_eq!(after.remaining_secs, before.remaining_secs);
    state.awareness[1].pos.x += 100.0;
    retarget_beam(&mut state, &context);
    assert!(matches!(state.beam, BeamState::Cooldown { .. }));
    state.awareness.clear();
    retarget_beam(&mut state, &context);
    assert!(state.route.is_none());
}

#[test]
fn closing_a_barrier_immediately_stops_a_turret() {
    let mut fixture = Fixture::new("turret");
    let origin = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let kind = BarrierKindId(0);
    let x = (origin.x + target.x) / 2.0;
    let kinds = BarrierKindTable::from_ids(vec!["shield".into()]).expect("barrier catalog rejected");
    fixture.collision_world = CollisionWorld::from_map_layout(
        &MapLayout {
            barriers: vec![Barrier {
                x1: x,
                x2: x,
                z1: origin.z - 4.0,
                z2: origin.z + 4.0,
                y: 0.0,
                height: WALL_HEIGHT,
                width: 0.1,
                level: 0,
                levels: 1,
                kind,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        &kinds,
    );
    let mut state = info("turret");
    state.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut context = fixture.context("turret", origin);
    let opened = [kind];
    context.open_barriers = &opened;
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
    context.open_barriers = &[];
    retarget_beam(&mut state, &context);
    assert!(matches!(state.beam, BeamState::Cooldown { .. }));
    context.open_barriers = &opened;
    assert!(decide_stationary_actor(&mut state, &context).is_none());
    let cooldown = context
        .kind_config
        .attack
        .beam()
        .expect("turret fires no beam")
        .cooldown_secs;
    tick_runtime_state(&mut state, origin, cooldown + TICK_SECS, context.kind_config, &[]);
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
}

fn actor_app(kind: &str, health: f32) -> (App, Entity, UnboundedReceiver<ServerToClient>) {
    let fixture = Fixture::new(kind);
    let origin = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut app = App::new();
    app.insert_resource(fixture.graphs)
        .insert_resource(fixture.territories)
        .insert_resource(fixture.carriers)
        .insert_resource(fixture.collision_world)
        .insert_resource(fixture.gameplay)
        .insert_resource(fixture.server)
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<MapItems>()
        .init_resource::<PlateState>()
        .init_resource::<ServerTick>()
        .init_resource::<Time>()
        .init_resource::<PendingExplosions>()
        .insert_resource(Invincibility(false))
        .add_systems(
            Update,
            (super::tick::actors_behavior_system, actors_beam_damage_system).chain(),
        );
    let actor = app.world_mut().spawn((ActorId(1), ActorMarker, origin)).id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(ActorId(1), ActorInfo::new(actor, 0, kind.into(), CarrierId::WORLD));
    let player = app
        .world_mut()
        .spawn((PlayerMarker, PlayerId(7), target, Health(health)))
        .id();
    let (sender, receiver) = unbounded_channel();
    let mut player_info = PlayerInfo::new(player, sender);
    player_info.connection.logged_in = true;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(PlayerId(7), player_info);
    (app, player, receiver)
}

fn turret_step(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(1.0 / 30.0));
    app.world_mut().resource_mut::<ServerTick>().0 += 1;
    app.update();
}

#[test]
fn turret_fires_over_cover_below_its_gun_despite_its_lower_body_center() {
    let (mut app, player, _) = actor_app("turret", 5000.0);
    let actor = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("turret missing")
        .entity;
    let actor_pos = *app.world().get::<Position>(actor).expect("turret position missing");
    let player_pos = *app.world().get::<Position>(player).expect("player position missing");
    let gameplay = app.world().resource::<GameplayConfig>();
    let turret = gameplay.expect_actor("turret");
    let target_y = gameplay.player.physics().hitbox_center_y(player_pos.y);
    let body_ray_y = (turret.physics().hitbox_center_y(actor_pos.y) + target_y) / 2.0;
    let gun_ray_y = (actor_pos.y + turret.beam_origin_height() + target_y) / 2.0;
    let cover_height = (body_ray_y + gun_ray_y) / 2.0 - actor_pos.y;
    let wall_x = (actor_pos.x + player_pos.x) / 2.0;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: wall_x,
            z1: actor_pos.z - 2.0,
            x2: wall_x,
            z2: actor_pos.z + 2.0,
            width: 0.1,
            y: actor_pos.y,
            height: cover_height,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()));
    turret_step(&mut app);
    assert!(app.world().get::<Health>(player).expect("player health missing").0 < 5000.0);
}

#[test]
fn peace_stops_attacks_and_targeting_until_disabled_for_every_actor_kind() {
    for kind in ["turret", "zapper", "hybrid", "bruiser", "scuttler"] {
        let (mut app, player, _) = actor_app(kind, 5000.0);
        turret_step(&mut app);
        let health = app.world().get::<Health>(player).expect("player health missing").0;
        app.world_mut().resource_mut::<ActorMap>().set_peaceful(true);
        for _ in 0..30 {
            turret_step(&mut app);
            let info = app
                .world()
                .resource::<ActorMap>()
                .get(&ActorId(1))
                .expect("actor missing");
            assert!(info.awareness.is_empty(), "{kind} noticed a player during peace");
            assert!(info.beam.target().is_none(), "{kind} fired during peace");
            assert!(!matches!(info.mode, ActorMode::Engage { .. } | ActorMode::Evade { .. }));
            assert_eq!(
                app.world().get::<Health>(player).expect("player health missing").0,
                health
            );
        }
        app.world_mut().resource_mut::<ActorMap>().set_peaceful(false);
        turret_step(&mut app);
        let info = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing");
        assert!(
            !info.awareness.is_empty(),
            "{kind} failed to notice players after peace"
        );
        if ["turret", "zapper", "hybrid"].contains(&kind) {
            assert!(info.beam.target().is_some(), "{kind} failed to resume firing");
        }
    }
}

#[test]
fn turret_holds_long_burst_and_stops_when_player_disconnects() {
    let (mut app, player, mut receiver) = actor_app("turret", 5000.0);
    for _ in 0..120 {
        turret_step(&mut app);
    }
    let health = app.world().get::<Health>(player).expect("player health missing").0;
    assert!((health - 3000.0).abs() < 0.1);
    let actor = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("turret missing");
    assert!(actor.route.is_none());
    assert_eq!(actor.beam.target(), Some(PlayerId(7)));
    let mut targets = Vec::new();
    while let Ok(ServerToClient::Send(message)) = receiver.try_recv() {
        if let ServerMessage::ActorBeam(cue) = message {
            targets.push(cue.beam.map(|beam| beam.target));
        }
    }
    assert_eq!(targets, vec![Some(PlayerId(7))]);
    let cooldown = app
        .world()
        .resource::<ServerGameplayConfig>()
        .expect_actor("turret")
        .attack
        .beam()
        .expect("turret fires no beam")
        .cooldown_secs;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(7), 2.0);
    turret_step(&mut app);
    assert_eq!(
        app.world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("turret missing")
            .beam,
        BeamState::Cooldown {
            remaining_secs: cooldown
        }
    );
    assert_eq!(
        app.world().get::<Health>(player).expect("player health missing").0,
        health
    );
}

#[test]
fn turret_repeats_bursts_with_a_damage_free_cooldown_and_transition_cues() {
    let (mut app, player, mut receiver) = actor_app("turret", 50000.0);
    let attack = app
        .world()
        .resource::<ServerGameplayConfig>()
        .expect_actor("turret")
        .attack
        .beam()
        .expect("turret beam config missing");
    let total_ticks = ((attack.duration_secs + attack.cooldown_secs) / TICK_SECS).ceil() as u32 + 5;
    let mut previous_health = 50000.0;
    let mut cooldown_ticks = 0;
    for _ in 0..total_ticks {
        turret_step(&mut app);
        let health = app.world().get::<Health>(player).expect("player health missing").0;
        let actor = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("turret missing");
        if actor.beam.target().is_none() {
            cooldown_ticks += 1;
            assert_eq!(health, previous_health);
        } else {
            assert!(health < previous_health);
        }
        previous_health = health;
    }
    let expected_cooldown_ticks = (attack.cooldown_secs / TICK_SECS).ceil() as u32;
    assert!((expected_cooldown_ticks..=expected_cooldown_ticks + 1).contains(&cooldown_ticks));
    let mut cues = Vec::new();
    while let Ok(ServerToClient::Send(message)) = receiver.try_recv() {
        if let ServerMessage::ActorBeam(cue) = message {
            cues.push(cue);
        }
    }
    assert_eq!(cues.len(), 3);
    let first = cues[0].beam.expect("first burst missing");
    assert!(cues[1].beam.is_none());
    let second = cues[2].beam.expect("second burst missing");
    assert_eq!(first.target, second.target);
    assert_ne!(first.started_tick, second.started_tick);
    let actual_duration = (cues[1].tick - cues[0].tick) as f32 * TICK_SECS;
    assert!((actual_duration - attack.duration_secs).abs() <= TICK_SECS);
}

#[test]
fn active_beams_retarget_disconnected_players_before_the_next_navigation_decision() {
    for kind in ["turret", "zapper", "hybrid"] {
        let (mut app, player, _) = actor_app(kind, 5000.0);
        turret_step(&mut app);
        let first = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .beam
            .snapshot()
            .expect("first burst missing");
        let pos = *app.world().get::<Position>(player).expect("player position missing");
        let next = app
            .world_mut()
            .spawn((PlayerMarker, PlayerId(8), pos, Health(5000.0)))
            .id();
        let (sender, _receiver) = unbounded_channel();
        let mut info = PlayerInfo::new(next, sender);
        info.connection.logged_in = true;
        app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(8), info);
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .disconnect(&PlayerId(7), 2.0);
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&ActorId(1))
            .expect("actor missing")
            .decision_timer = 1.0;
        turret_step(&mut app);
        let after = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .beam
            .snapshot()
            .expect("burst ended during retarget");
        assert_eq!(after.target, PlayerId(8), "{kind}");
        assert_eq!(after.started_tick, first.started_tick, "{kind}");
        assert!(after.remaining_secs < first.remaining_secs, "{kind}");
    }
}
