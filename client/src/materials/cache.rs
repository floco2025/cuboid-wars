use std::collections::HashMap;

use bevy::prelude::*;

use super::MaterialTextures;
use crate::config::MaterialDef;

#[derive(Default)]
pub struct MaterialHandleCache {
    standard: HashMap<String, Handle<StandardMaterial>>,
}

impl MaterialHandleCache {
    pub fn standard(
        &mut self,
        id: &str,
        material_def: &MaterialDef,
        textures: &mut MaterialTextures,
        materials: &mut Assets<StandardMaterial>,
        anisotropy: u16,
        mipmaps_enabled: bool,
    ) -> Handle<StandardMaterial> {
        self.standard
            .entry(id.to_owned())
            .or_insert_with(|| materials.add(material_def.standard_material(textures, anisotropy, mipmaps_enabled)))
            .clone()
    }
}
