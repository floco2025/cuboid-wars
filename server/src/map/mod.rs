mod barriers;
mod bridge_bounds;
mod bridges;
mod definition;
mod edges;
mod floors;
pub(crate) mod generation;
mod grid;
mod light_cycle;
mod lights;
mod mask;
pub(crate) mod material_rules;
mod plugin;
mod pressure_plates;
mod ramps;
#[cfg(test)]
mod relay_tests;
mod resources;
mod segments;
mod trim;
mod walls;
mod weather;

pub(crate) use edges::{CellSide, has_edge_on_cell_side};

pub use common::protocol::PlateState;
pub use generation::{GeneratedMap, generate_map};
pub use grid::grid_coords_from_position;
pub use light_cycle::{LightState, light_cycle_is_running, light_cycle_system, light_preset_from_str};
pub use plugin::map_plugin;
pub use pressure_plates::pressure_plates_system;
pub use resources::{
    ActorSpawnZone, CarrierGrid, Cell, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlacedItem, PlayerSpawnZone,
    PressurePlateRuntime,
};
pub use weather::{WeatherState, weather_needs_tick, weather_system};
