mod cover;
mod surface;

pub(in crate::map) use cover::TerrainCover;
pub use surface::{TerrainMarker, terrain_spawn_system};
