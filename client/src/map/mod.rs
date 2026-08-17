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
    map_level_focus_visibility_system, map_spawn_geometry_system, map_wall_light_emissive_system,
    setup_scene_lighting_system,
};
pub use resources::{DebugColors, LevelFocusEnabled};
pub use spawn::{
    GroundMarker, MapGeometryBatch, MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker, batch_floor,
    batch_ramp, batch_wall, spawn_wall_light_from_layout, wall_light_flicker_system,
};

mod plugin;

pub use plugin::{map_plugin, sky_weather_plugin};
