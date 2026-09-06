use bevy::prelude::Resource;
use common::protocol::CarrierId;

use crate::map::MapConfig;

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

    #[must_use]
    pub fn get(&self, carrier: CarrierId) -> &NavGraph {
        self.0
            .get(usize::from(carrier.0))
            .expect("carrier named by an actor spawn zone has no navigation graph")
    }
}
