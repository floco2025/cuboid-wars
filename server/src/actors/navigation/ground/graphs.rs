use bevy::prelude::{DetectChanges, Res, ResMut, Resource};
use common::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{BarrierId, BridgeId, CarrierId, MapLayout, MapSettings, PlateState},
};

use crate::{config::ServerGameplayConfig, map::MapConfig};

use super::{NavGraph, ladders::LadderClimber};

// One navigation graph per carrier grid.
#[derive(Resource)]
pub struct NavGraphs(Vec<NavGraph>);

impl NavGraphs {
    pub(super) fn iter(&self) -> impl Iterator<Item = (CarrierId, &NavGraph)> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, graph)| (CarrierId(index as u16), graph))
    }
    #[must_use]
    pub fn new(map: &MapConfig) -> Self {
        let graphs = map
            .grids
            .iter()
            .enumerate()
            .map(|(index, grid)| {
                assert_eq!(
                    usize::from(grid.carrier.0),
                    index,
                    "map config grids are not in carrier order"
                );
                NavGraph::new(grid)
            })
            .collect();
        Self(graphs)
    }

    pub fn add_ladder_routes(&mut self, layout: &MapLayout, settings: &MapSettings, config: &ServerGameplayConfig) {
        if !config
            .actors
            .kinds
            .values()
            .any(|actor| actor.character.can_use_ladders)
        {
            return;
        }
        // Barriers some plate opens: a route may plan through them and
        // wait for physics to let the actor pass.
        let passable: Vec<BarrierId> = layout
            .barriers
            .iter()
            .filter(|barrier| barrier.switch.is_some())
            .map(|barrier| barrier.id)
            .collect();
        for (index, graph) in self.0.iter_mut().enumerate() {
            let carrier = CarrierId(index as u16);
            let local = MapLayout {
                walls: on_carrier(&layout.walls, carrier, |wall| &mut wall.carrier),
                floors: on_carrier(&layout.floors, carrier, |floor| &mut floor.carrier),
                ramps: on_carrier(&layout.ramps, carrier, |ramp| &mut ramp.carrier),
                barriers: on_carrier(&layout.barriers, carrier, |barrier| &mut barrier.carrier),
                ladders: on_carrier(&layout.ladders, carrier, |ladder| &mut ladder.carrier),
                ..Default::default()
            };
            if local.ladders.is_empty() {
                continue;
            }
            let world = CollisionWorld::from_map_layout(&local);
            let carriers = Carriers::default();
            for (kind, actor) in &config.actors.kinds {
                if !actor.character.can_use_ladders {
                    continue;
                }
                let movement = settings.movement.expect_actor(kind);
                let climber = LadderClimber {
                    delta: config.network.tick_secs(),
                    collision_world: &world,
                    map_settings: settings,
                    physics: actor.character.physics(),
                    passable_kinds: &passable,
                    carriers: &carriers,
                };
                let links = local
                    .ladders
                    .iter()
                    .flat_map(|ladder| {
                        graph.build_ladder_links(ladder, &climber, [movement.roam_speed, movement.active_speed])
                    })
                    .collect();
                graph.ladder_routes.insert(kind.clone(), links);
            }
        }
    }

    #[must_use]
    pub fn get(&self, carrier: CarrierId) -> &NavGraph {
        self.0
            .get(usize::from(carrier.0))
            .expect("carrier named by an actor spawn zone has no navigation graph")
    }

    pub fn set_powered_bridges(&mut self, powered: &[BridgeId]) {
        for graph in &mut self.0 {
            graph.set_powered_bridges(powered);
        }
    }
}

// Applies the powered bridges to the navigation graphs, as
// `powered_bridges_sync_system` does to the collision world, so the
// behaviour that follows plans over this tick's bridges.
pub fn nav_bridges_sync_system(plates: Res<PlateState>, mut graphs: ResMut<NavGraphs>) {
    if plates.is_changed() {
        graphs.set_powered_bridges(&plates.powered_bridges);
    }
}

// The records on `carrier`, retagged as world records so a collision world
// built from them stands in that carrier's own frame.
fn on_carrier<T: Copy>(records: &[T], carrier: CarrierId, carrier_of: impl Fn(&mut T) -> &mut CarrierId) -> Vec<T> {
    records
        .iter()
        .copied()
        .filter_map(|mut record| {
            let tag = carrier_of(&mut record);
            if *tag != carrier {
                return None;
            }
            *tag = CarrierId::WORLD;
            Some(record)
        })
        .collect()
}
