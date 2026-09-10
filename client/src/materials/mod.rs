mod cache;
#[cfg(test)]
#[path = "tests/gltf.rs"]
mod gltf_tests;
mod grass;
mod mipmaps;
mod portal_clip;
mod standard;

pub use cache::MaterialHandleCache;
pub use grass::{GrassMaterial, GrassMaterialPlugin, GrassWindExtension};
pub use mipmaps::generate_material_mipmaps_system;
pub use portal_clip::{PortalClipExtension, PortalClipMaterial, PortalClipMaterialPlugin, portal_clip_material};
