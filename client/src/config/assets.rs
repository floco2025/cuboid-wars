use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use common::{
    material_rules::{FaceMaterials, MaterialRules},
    protocol::{Floor, ItemType, Ramp, Wall},
};
use serde::Deserialize;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    pub version: u32,
    materials: HashMap<String, MaterialDef>,
    #[serde(flatten)]
    rules: MaterialRules,
    models: Models,
    skybox: SkyboxDef,
    sounds: HashMap<String, String>,
}

impl AssetSet {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/assets.json")))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn material_ids_for_floor(&self, floor: &Floor) -> FaceMaterials {
        self.rules.materials_for_floor(floor)
    }

    pub fn material_ids_for_ramp_top(&self, ramp: &Ramp) -> FaceMaterials {
        self.rules.materials_for_ramp_top(ramp)
    }

    pub fn material_ids_for_ramp_side(&self, ramp: &Ramp) -> FaceMaterials {
        self.rules.materials_for_ramp_side(ramp)
    }

    pub fn material_for_item(&self, item_type: ItemType) -> &MaterialDef {
        self.material(self.rules.material_for_item(item_type))
    }

    pub fn material_ids_for_wall(&self, wall: &Wall) -> FaceMaterials {
        self.rules.materials_for_wall(wall)
    }

    pub fn material_by_id(&self, id: &str) -> &MaterialDef {
        self.material(id)
    }

    pub fn player_model(&self) -> &ModelDef {
        &self.models.player
    }

    pub fn actor_model(&self) -> &ModelDef {
        &self.models.actor
    }

    pub fn wall_light_model(&self) -> &WallLightModelDef {
        &self.models.wall_light
    }

    pub fn skybox(&self) -> &SkyboxDef {
        &self.skybox
    }

    pub fn sound(&self, id: &str) -> &str {
        self.sounds
            .get(id)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("asset set is missing sound {id:?}"))
    }

    fn material(&self, id: &str) -> &MaterialDef {
        self.materials
            .get(id)
            .unwrap_or_else(|| panic!("asset set is missing material {id:?}"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialDef {
    pub(crate) textures: TextureDef,
    #[serde(default)]
    pub tile_size: Option<f32>,
    pub metallic: f32,
    #[serde(rename = "roughness")]
    pub perceptual_roughness: f32,
    #[serde(default)]
    pub(crate) repeat: bool,
    #[serde(default)]
    pub(crate) data_textures_linear: bool,
    #[serde(default)]
    pub(crate) base_color: Option<String>,
    #[serde(default)]
    pub(crate) emissive: Option<String>,
    #[serde(default)]
    pub(crate) emissive_strength: Option<f32>,
}

impl MaterialDef {
    #[must_use]
    pub fn tile_size(&self) -> f32 {
        self.tile_size.unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TextureDef {
    pub(crate) base_color: String,
    pub(crate) normal: String,
    pub(crate) occlusion: String,
    pub(crate) metallic_roughness: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelDef {
    pub scene: String,
    pub scale: f32,
    #[serde(default)]
    pub visual_y_offset: f32,
    #[serde(default)]
    pub animation_speed: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub scene: String,
    pub scale: f32,
    pub inward_offset: f32,
    pub brightness: f32,
    pub range: f32,
    pub radius: f32,
    pub emissive_luminance: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkyboxDef {
    pub cross_image: String,
    pub brightness: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct Models {
    player: ModelDef,
    actor: ModelDef,
    wall_light: WallLightModelDef,
}
