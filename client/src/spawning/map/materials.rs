use std::collections::HashMap;

use bevy::prelude::*;

use crate::config::MaterialDef;

#[derive(Default)]
pub struct MapMaterialCache {
    standard: HashMap<String, Handle<StandardMaterial>>,
}

impl MapMaterialCache {
    pub fn standard(
        &mut self,
        id: &str,
        material_def: &MaterialDef,
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        self.standard
            .entry(id.to_owned())
            .or_insert_with(|| materials.add(material_def.standard_material(asset_server)))
            .clone()
    }
}
