mod burn;
mod material;
mod mesh;
mod patch;
mod streaming;

#[cfg(test)]
mod tests;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub(in crate::map) use patch::GrassPatch;
pub(in crate::map) use streaming::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource};
pub use streaming::{GrassChunkMarker, GrassChunks, grass_chunks_reset_system, grass_streaming_system};
