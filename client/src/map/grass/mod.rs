mod burn;
mod mesh;
mod spawn;

#[cfg(test)]
mod tests;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub(super) use mesh::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_scatter_mesh};
pub(super) use spawn::grass_material;
pub use spawn::{TerrainMarker, terrain_spawn_system};
