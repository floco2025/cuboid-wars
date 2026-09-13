mod burn;
mod mesh;
mod spawn;

#[cfg(test)]
mod tests;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub(super) use mesh::GrassLod;
pub use spawn::{TerrainMarker, terrain_spawn_system};
