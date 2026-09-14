use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::{FaceMaterials, TERRAIN_MATERIAL};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct TextureSettings {
    // An id in the client's `assets.json::materials`; the server never resolves it.
    pub material: String,
    pub portalable: bool,
}

pub fn validate_texture_catalog(textures: &BTreeMap<String, TextureSettings>, path: &str) -> Result<()> {
    for (alias, texture) in textures {
        ensure!(!alias.trim().is_empty(), "{path} contains an empty texture alias");
        ensure!(
            alias != TERRAIN_MATERIAL,
            "{path}.{alias}: the procedural terrain alias cannot be redefined by a map"
        );
        ensure!(!texture.material.trim().is_empty(), "{path}.{alias}.material is empty");
    }
    Ok(())
}

pub fn validate_texture_materials(
    materials: &FaceMaterials,
    textures: &BTreeMap<String, TextureSettings>,
    path: &str,
) -> Result<()> {
    for (face, alias) in [
        ("top", &materials.top),
        ("bottom", &materials.bottom),
        ("north", &materials.north),
        ("south", &materials.south),
        ("east", &materials.east),
        ("west", &materials.west),
    ] {
        ensure!(
            textures.contains_key(alias),
            "{path}.{face}: texture alias {alias:?} is absent from the host map's textures"
        );
    }
    Ok(())
}
