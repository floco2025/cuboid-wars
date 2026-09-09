use bevy::prelude::Resource;
use common::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{CarrierId, MapLayout, MapSettings, PlatePurpose},
};

use crate::{config::ServerGameplayConfig, map::MapConfig};

use super::{NavGraph, ladders::LadderClimber};

// One navigation graph per grid, indexed by carrier id: an actor navigates
// the grid of the carrier its zone is on, in that carrier's frame.
#[derive(Resource)]
pub struct NavGraphs(Vec<NavGraph>);

impl NavGraphs {
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
        let (barrier_kinds, _) = settings
            .kind_tables()
            .expect("map kind tables invalid after validation");
        let passable: Vec<_> = layout
            .pressure_plates
            .iter()
            .filter_map(|plate| match plate.purpose {
                PlatePurpose::Barrier(kind) => Some(kind),
                _ => None,
            })
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
            let world = CollisionWorld::from_map_layout(&local, &barrier_kinds);
            let carriers = Carriers::default();
            for (kind, actor) in &config.actors.kinds {
                if !actor.character.can_use_ladders {
                    continue;
                }
                let movement = settings.movement.expect_actor(kind);
                let climber = LadderClimber {
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
