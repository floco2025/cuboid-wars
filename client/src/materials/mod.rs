mod cache;
mod field;
mod flag;
#[cfg(test)]
#[path = "tests/gltf.rs"]
mod gltf_tests;
mod grass;
mod mipmaps;
mod portal_clip;
mod sky;
mod standard;
mod terrain;
mod textures;
mod tree;

pub use cache::MaterialHandleCache;
pub use field::{FieldExtension, FieldMaterial, FieldMaterialPlugin, field_material};
pub use flag::{FlagMaterial, FlagMaterialPlugin, FlagWindExtension};
pub use grass::{GrassMaterial, GrassMaterialPlugin, GrassWindExtension};
pub(crate) use mipmaps::MaterialMipmapState;
pub use portal_clip::{PortalClipExtension, PortalClipMaterial, PortalClipMaterialPlugin, portal_clip_material};
pub use sky::{ProceduralSkyMaterial, ProceduralSkyMaterialPlugin};
pub use terrain::{TerrainMaterial, TerrainMaterialPlugin, terrain_material};
pub use textures::{MaterialTextures, material_textures_plugin};
pub use tree::{TreeMaterial, TreeMaterialPlugin, TreeWindExtension};
