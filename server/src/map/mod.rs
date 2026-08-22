mod barriers;
mod definition;
mod edges;
mod floors;
pub(crate) mod generation;
mod grid;
mod lights;
mod mask;
pub(crate) mod material_rules;
mod plugin;
mod pressure_plates;
mod ramps;
mod resources;
mod segments;
mod trim;
mod walls;
mod weather;

pub(crate) use edges::{CellSide, has_edge_on_cell_side};

pub use common::physics::OpenBarrierKinds;
pub use generation::generate_map;
pub use grid::grid_coords_from_position;
pub use plugin::map_plugin;
pub use pressure_plates::compute_open_barrier_kinds_system;
pub use resources::{
    ActorSpawnZone, Cell, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlacedItem, PlayerSpawnZone, PressurePlateRuntime,
};
pub use weather::{CurrentLighting, WeatherState, weather_system};
