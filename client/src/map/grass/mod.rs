mod burn;
mod mesh;
mod spawn;
mod streaming;

#[cfg(test)]
mod tests;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub(super) use spawn::GrassPatch;
pub use spawn::{TerrainMarker, terrain_spawn_system};
pub(super) use streaming::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource};
pub use streaming::{GrassChunkMarker, GrassChunks, grass_chunks_reset_system, grass_streaming_system};
