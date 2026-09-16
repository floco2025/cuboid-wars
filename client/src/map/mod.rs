mod celestial;
mod clouds;
mod grass;
mod grounds;
mod rendering;
mod resources;
mod rocks;
mod sky_probe;
pub mod spawn;
mod terrain;
mod trees;
mod weather_surfaces;

pub use celestial::setup_scene_lighting_system;
pub(crate) use grass::GrassBurn;
pub use grass::{
    GrassChunkMarker, GrassChunks, GrassSources, grass_burn_system, grass_chunk_finish_system,
    grass_sources_reset_system, grass_streaming_system, setup_grass_materials_system,
};
pub use rendering::{
    added_map_level_visibility_system, map_level_focus_visibility_system, map_spawn_geometry_system,
    map_wall_light_emissive_system, update_focused_map_level_system,
};
pub use resources::{DebugColorMode, DebugColors, FocusedMapLevel, LevelFocusEnabled};
pub use spawn::{
    GroundMarker, LadderMarker, MapGeometryBatch, MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker,
    batch_floor, batch_ramp, batch_wall, spawn_ladder_from_layout, spawn_wall_light_from_layout, tiled_cuboid,
    wall_light_flicker_system,
};
pub use terrain::{TerrainMarker, terrain_spawn_system};

mod plugin;

pub use plugin::{map_plugin, sky_weather_plugin};
