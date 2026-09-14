mod burn;
mod material;
mod mesh;
mod patch;
mod sources;
mod streaming;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(in crate::map) mod fixtures;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub(crate) use material::GrassMaterials;
pub use material::setup_grass_materials_system;
pub(in crate::map) use patch::GrassPatch;
pub(in crate::map) use sources::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource};
pub use sources::{GrassSources, grass_sources_reset_system};
pub use streaming::{GrassChunkMarker, GrassChunks, grass_chunk_finish_system, grass_streaming_system};
