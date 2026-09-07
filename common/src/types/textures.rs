use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::FaceMaterials;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct TextureSettings {
    pub portalable: bool,
}

pub fn validate_texture_catalog(textures: &BTreeMap<String, TextureSettings>, path: &str) -> Result<()> {
    for alias in textures.keys() {
        ensure!(!alias.trim().is_empty(), "{path} contains an empty texture alias");
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
