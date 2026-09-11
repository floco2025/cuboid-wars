pub(super) use bevy::prelude::*;
pub(super) use rand::{SeedableRng, rngs::StdRng};
pub(super) use std::time::Duration;
pub(super) use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

pub(super) use super::super::{
    controllers::{
        decide_beam_actor, decide_contact_actor, decide_contact_beam_actor, decide_stationary_actor, retarget_beam,
    },
    perception::{PlayerState, update_awareness},
    tick::{actors_behavior_system, drop_route_onto_lost_bridge, shake_loose, tick_runtime_state},
    transitions::{BehaviorContext, EVADE_REPLAN_INTERVAL_SECS, enter_evade, keep_or_install_engagement_route},
};
pub(super) use crate::{
    actors::{
        ActorCharacter, ActorInfo, ActorMap, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph, NavGraphs, NavWaypoint, WaypointKind},
        test_kinds::{self, BEAM, CONTACT, CONTACT_BEAM, IMMOVABLE, KINDS},
    },
    combat::{PendingExplosions, actors_beam_damage_system},
    config::ServerGameplayConfig,
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    players::{Invincibility, PlayerInfo, PlayerMap},
    test_geometry::{CELL, LEVEL_HEIGHT, WALL_HEIGHT, geometry},
};
pub(super) use common::{
    config::GameplayConfig,
    constants::TICK_SECS,
    map::{CarrierPose, Carriers, MapGeometry},
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        ActorBeam, ActorId, ActorMarker, Barrier, BarrierKindId, BridgeId, Carrier, CarrierId, Health, MapItems,
        MapLayout, PlateState, PlayerId, PlayerMarker, Position, ServerMessage, ServerTick, Wall,
    },
};

// A 12x5 all-floor grid with one zone at (1, 2). With a carrier, the zone's
// grid is nested on a static carrier resting at `rest`, so `pos` is in the
// carrier's frame and the world is `pose()` away.
pub(crate) struct Fixture {
    pub(crate) graphs: NavGraphs,
    pub(crate) territories: ActorTerritories,
    pub(crate) collision_world: CollisionWorld,
    pub(crate) gameplay: GameplayConfig,
    pub(crate) server: ServerGameplayConfig,
    pub(crate) geometry: MapGeometry,
    pub(crate) carrier: CarrierId,
    pub(crate) carriers: Carriers,
}

impl Fixture {
    pub(crate) fn new(kind: &str) -> Self {
        Self::with_world(kind, CollisionWorld::from_map_layout(&MapLayout::default()))
    }

    pub(crate) fn with_world(kind: &str, collision_world: CollisionWorld) -> Self {
        Self::with_levels_and_world(kind, 1, collision_world)
    }

    pub(crate) fn with_levels(kind: &str, level_count: usize) -> Self {
        Self::with_levels_and_world(
            kind,
            level_count,
            CollisionWorld::from_map_layout(&MapLayout::default()),
        )
    }

    pub(crate) fn with_levels_and_world(kind: &str, level_count: usize, collision_world: CollisionWorld) -> Self {
        Self::build(kind, level_count, collision_world, None)
    }

    pub(crate) fn with_carrier(kind: &str, rest: Position) -> Self {
        Self::build(
            kind,
            1,
            CollisionWorld::from_map_layout(&MapLayout::default()),
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
                switch_inverted: false,

                carrier,
                level: 0,
                cols: [1, 2],
                rows: [2, 3],
                kind: kind.to_owned(),
                count: 1,
                respawn_secs: None,
                switch: None,
            }],
            ..MapConfig::for_grid(levels(level_count), geometry)
        };
        let mut layout = MapLayout::default();
        if let Some(rest) = rest {
            map.grids.push(CarrierGrid::new(carrier, geometry, levels(1)));
            layout.carriers.push(Carrier {
                switch_inverted: false,

                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: rest,
                to: rest,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
            });
        }
        let carriers = Carriers::from_layout(&layout);
        let graphs = NavGraphs::new(&map);
        let server = test_kinds::server_config();
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

    pub(crate) fn graph(&self) -> &NavGraph {
        self.graphs.get(self.carrier)
    }

    pub(crate) fn pose(&self) -> CarrierPose {
        self.carriers.pose(self.carrier)
    }

    // Off-centre inside the cell, in the zone's grid frame.
    pub(crate) fn pos(&self, col: i32, row: i32) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(col) + 2.0,
            y: 0.0,
            z: self.geometry.cell_to_world_z(row) + 2.0,
        }
    }

    pub(super) fn context(&self, kind: &str, pos: Position) -> BehaviorContext<'_> {
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

pub(crate) fn info(kind: &str) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, kind.to_owned(), CarrierId::WORLD)
}

pub(crate) fn aware(
    id: u32,
    pos: Position,
    support: CharacterSupport,
    visible: bool,
) -> crate::actors::resources::AwarePlayer {
    crate::actors::resources::AwarePlayer {
        id: PlayerId(id),
        pos,
        support,
        visible,
        forget_remaining_secs: 10.0,
        attack_anchor: None,
    }
}

pub(crate) fn route_through(waypoints: &[Position], fixture: &Fixture) -> ActorRoute {
    let destination = *waypoints.last().expect("route has a destination");
    ActorRoute {
        waypoints: waypoints.iter().copied().map(NavWaypoint::walk).collect(),
        destination,
        destination_node: fixture
            .graph()
            .nearest_node_for_position(&destination)
            .expect("destination nav node"),
    }
}

pub(crate) fn tick_route(info: &mut ActorInfo, pos: Position, fixture: &Fixture) {
    tick_runtime_state(info, pos, 0.1, fixture.server.expect_actor(CONTACT), &[]);
}
