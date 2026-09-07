use bevy::prelude::Resource;
use common::{
    map::Carriers,
    physics::{CharacterEnvironment, CollisionWorld, LadderMode},
    protocol::{CarrierId, MapLayout, MapSettings, PlatePurpose},
};

use crate::{config::ServerGameplayConfig, map::MapConfig};

use super::NavGraph;

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
                walls: layout
                    .walls
                    .iter()
                    .filter(|wall| wall.carrier == carrier)
                    .map(|wall| {
                        let mut wall = *wall;
                        wall.carrier = CarrierId::WORLD;
                        wall
                    })
                    .collect(),
                floors: layout
                    .floors
                    .iter()
                    .filter(|floor| floor.carrier == carrier)
                    .map(|floor| {
                        let mut floor = *floor;
                        floor.carrier = CarrierId::WORLD;
                        floor
                    })
                    .collect(),
                ramps: layout
                    .ramps
                    .iter()
                    .filter(|ramp| ramp.carrier == carrier)
                    .map(|ramp| {
                        let mut ramp = *ramp;
                        ramp.carrier = CarrierId::WORLD;
                        ramp
                    })
                    .collect(),
                barriers: layout
                    .barriers
                    .iter()
                    .filter(|barrier| barrier.carrier == carrier)
                    .map(|barrier| {
                        let mut barrier = *barrier;
                        barrier.carrier = CarrierId::WORLD;
                        barrier
                    })
                    .collect(),
                ladders: layout
                    .ladders
                    .iter()
                    .filter(|ladder| ladder.carrier == carrier)
                    .map(|ladder| {
                        let mut ladder = *ladder;
                        ladder.carrier = CarrierId::WORLD;
                        ladder
                    })
                    .collect(),
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
                let environment = CharacterEnvironment {
                    collision_world: &world,
                    gravity: settings.movement.gravity,
                    physics: actor.character.physics(),
                    ladder_mode: LadderMode::Disabled,
                    ladder_climb_ratio: settings.movement.ladder_climb_ratio,
                    passable_kinds: &passable,
                    portals: None,
                    carriers: &carriers,
                };
                let links = local
                    .ladders
                    .iter()
                    .flat_map(|ladder| {
                        graph.build_ladder_links(ladder, &environment, [movement.roam_speed, movement.active_speed])
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
