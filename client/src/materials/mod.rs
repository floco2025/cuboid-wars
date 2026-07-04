mod cache;
mod grass;
mod mipmaps;
mod standard;

pub use cache::MaterialHandleCache;
pub use grass::{GrassMaterial, GrassMaterialPlugin, GrassWindExtension};
pub use mipmaps::generate_material_mipmaps_system;
