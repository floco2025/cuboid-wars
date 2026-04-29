use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};
use common::{
    assets::AssetRules,
    protocol::{Floor, ItemType, Ramp, Wall},
};
use serde::Deserialize;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    pub version: u32,
    materials: HashMap<String, MaterialDef>,
    #[serde(flatten)]
    rules: AssetRules,
    models: Models,
    skybox: SkyboxDef,
    sounds: HashMap<String, String>,
}

impl AssetSet {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.json")))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn material_for_floor(&self, floor: &Floor) -> &MaterialDef {
        self.material(self.rules.material_for_floor(floor))
    }

    pub fn material_for_ramp_top(&self, ramp: &Ramp) -> &MaterialDef {
        self.material(self.rules.material_for_ramp_top(ramp))
    }

    pub fn material_for_ramp_side(&self, ramp: &Ramp) -> &MaterialDef {
        self.material(self.rules.material_for_ramp_side(ramp))
    }

    pub fn material_for_item(&self, item_type: ItemType) -> &MaterialDef {
        self.material(self.rules.material_for_item(item_type))
    }

    pub fn material_for_wall(&self, wall: &Wall) -> &MaterialDef {
        self.material(self.rules.material_for_wall(wall))
    }

    pub fn player_model(&self) -> &ModelDef {
        &self.models.player
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
    textures: TextureDef,
    #[serde(default)]
    pub tile_size: Option<f32>,
    pub metallic: f32,
    #[serde(rename = "roughness")]
    pub perceptual_roughness: f32,
    #[serde(default)]
    repeat: bool,
    #[serde(default)]
    data_textures_linear: bool,
    #[serde(default)]
    base_color: Option<String>,
    #[serde(default)]
    emissive: Option<String>,
    #[serde(default)]
    emissive_strength: Option<f32>,
}

impl MaterialDef {
    #[must_use]
    pub fn standard_material(&self, asset_server: &AssetServer) -> StandardMaterial {
        StandardMaterial {
            base_color_texture: Some(load_texture(
                asset_server,
                &self.textures.base_color,
                self.repeat,
                false,
            )),
            normal_map_texture: Some(load_texture(
                asset_server,
                &self.textures.normal,
                self.repeat,
                self.data_textures_linear,
            )),
            occlusion_texture: Some(load_texture(
                asset_server,
                &self.textures.occlusion,
                self.repeat,
                self.data_textures_linear,
            )),
            metallic_roughness_texture: Some(load_texture(
                asset_server,
                &self.textures.metallic_roughness,
                self.repeat,
                self.data_textures_linear,
            )),
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            ..default()
        }
    }

    #[must_use]
    pub fn standard_item_material(&self, asset_server: &AssetServer, item_color: Color) -> StandardMaterial {
        let mut material = self.standard_material(asset_server);
        if self.base_color.as_deref() == Some("item_type_color") {
            material.base_color = item_color;
        }
        if self.emissive.as_deref() == Some("item_type_color") {
            material.emissive = LinearRgba::from(item_color) * self.emissive_strength.unwrap_or(1.0);
        }
        material
    }

    #[must_use]
    pub fn tile_size(&self) -> f32 {
        self.tile_size.unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TextureDef {
    base_color: String,
    normal: String,
    occlusion: String,
    metallic_roughness: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelDef {
    pub scene: String,
    pub scale: f32,
    #[serde(default)]
    pub height_offset: f32,
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
    wall_light: WallLightModelDef,
}

fn load_texture(asset_server: &AssetServer, path: &str, repeat: bool, linear: bool) -> Handle<Image> {
    if !repeat && !linear {
        return asset_server.load(path.to_owned());
    }

    asset_server.load_with_settings(path.to_owned(), move |settings: &mut ImageLoaderSettings| {
        settings.is_srgb = !linear;
        if repeat {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                address_mode_w: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 8,
                ..default()
            });
        }
    })
}
