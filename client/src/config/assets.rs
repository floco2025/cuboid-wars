use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use common::{
    config::resolve_actor_inheritance,
    material_rules::{FaceMaterials, MaterialRules},
    protocol::{Floor, ItemType, Ramp, Wall},
};
use serde::Deserialize;

const SUPPORTED_VERSION: u32 = 1;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    pub version: u32,
    materials: HashMap<String, MaterialDef>,
    #[serde(flatten)]
    rules: MaterialRules,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
    models: GenericModels,
    skybox: SkyboxDef,
}

impl AssetSet {
    pub fn load_default() -> Result<Self> {
        let assets = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/client/assets.json"
        )))?;
        assets.validate()?;
        Ok(assets)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let mut value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        resolve_actor_inheritance(&mut value, "actors")
            .with_context(|| format!("resolving actor inheritance in {}", path.display()))?;
        serde_json::from_value(value).with_context(|| format!("failed to deserialize {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported asset config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        Ok(())
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
        &self.player.model
    }

    pub fn actor_model(&self, kind: &str) -> &ModelDef {
        &self.actor(kind).model
    }

    pub fn wall_light_model(&self) -> &WallLightModelDef {
        &self.models.wall_light
    }

    pub fn actor_explosion_effect(&self, kind: &str) -> &EffectDef {
        &self.actor(kind).explosion_effect
    }

    pub fn skybox(&self) -> &SkyboxDef {
        &self.skybox
    }

    pub fn player_sound(&self, name: &str) -> &str {
        self.player
            .sounds
            .get(name)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("asset set is missing player sound {name:?}"))
    }

    pub fn actor_sound(&self, kind: &str, name: &str) -> &str {
        self.actor(kind)
            .sounds
            .get(name)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("asset set is missing actor sound {kind:?}.{name:?}"))
    }

    fn actor(&self, kind: &str) -> &ActorAssets {
        self.actors
            .get(kind)
            .unwrap_or_else(|| panic!("asset set is missing actor kind {kind:?}"))
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
    pub(crate) linear_data_textures: bool,
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
    // Model bottom offset relative to the gameplay collider bottom.
    #[serde(default)]
    pub y_offset: f32,
    #[serde(default)]
    pub animation_speed: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub scene: String,
    pub scale: f32,
    pub offset_from_wall: f32,
    pub brightness: f32,
    pub range: f32,
    pub radius: f32,
    pub emissive_luminance: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EffectDef {
    pub image: String,
    pub columns: u32,
    pub rows: u32,
    pub first_frame: SpriteSheetFirstFrame,
    pub scale: f32,
    pub lifetime: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpriteSheetFirstFrame {
    UpperLeft,
    LowerRight,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkyboxDef {
    // Path to a cube-cross layout image used to derive the cubemap faces.
    pub image: String,
    pub brightness: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct PlayerAssets {
    model: ModelDef,
    sounds: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorAssets {
    model: ModelDef,
    sounds: HashMap<String, String>,
    explosion_effect: EffectDef,
}

#[derive(Debug, Clone, Deserialize)]
struct GenericModels {
    wall_light: WallLightModelDef,
}
