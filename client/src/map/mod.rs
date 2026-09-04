pub mod cubemap;
mod grass;
mod rendering;
mod resources;
pub mod skybox;
pub mod spawn;

pub(crate) use grass::GrassBurn;
pub use grass::{GrassMarker, grass_burn_system, grass_spawn_system};
pub(crate) use rendering::visual_focus_level;
pub use rendering::{
    added_map_level_visibility_system, map_level_focus_visibility_system, map_spawn_geometry_system,
    map_wall_light_emissive_system, setup_scene_lighting_system, update_focused_map_level_system,
};
pub use resources::{DebugColorMode, DebugColors, FocusedMapLevel, LevelFocusEnabled};
pub use spawn::{
    GroundMarker, LadderMarker, MapGeometryBatch, MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker,
    batch_floor, batch_ramp, batch_wall, spawn_ladder_from_layout, spawn_wall_light_from_layout, tiled_cuboid,
    wall_light_flicker_system,
};

mod plugin;

pub use plugin::{map_plugin, sky_weather_plugin};
