use std::collections::HashMap;

use bevy::prelude::*;

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
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        anisotropy: u16,
        mipmaps_enabled: bool,
    ) -> Handle<StandardMaterial> {
        self.standard
            .entry(id.to_owned())
            .or_insert_with(|| materials.add(material_def.standard_material(asset_server, anisotropy, mipmaps_enabled)))
            .clone()
    }
}
