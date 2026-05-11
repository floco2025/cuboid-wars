use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use common::{config::resolve_actor_inheritance, protocol::ItemType};
use serde::Deserialize;

const SUPPORTED_VERSION: u32 = 1;

#[derive(Resource, Debug, Clone)]
pub struct AssetSet {
    pub version: u32,
    materials: HashMap<String, MaterialDef>,
    aliases: HashMap<String, String>,
    item_materials: HashMap<String, String>,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
    models: GenericModels,
    skybox: SkyboxDef,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetSetFile {
    version: u32,
    materials: HashMap<String, MaterialDef>,
    #[serde(default)]
    aliases: HashMap<String, String>,
    item_materials: HashMap<String, String>,
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
        let file: AssetSetFile =
            serde_json::from_value(value).with_context(|| format!("failed to deserialize {}", path.display()))?;
        Ok(Self {
            version: file.version,
            materials: file.materials,
            aliases: file.aliases,
            item_materials: file.item_materials,
            player: file.player,
            actors: file.actors,
            models: file.models,
            skybox: file.skybox,
        })
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported asset config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        anyhow::ensure!(
            self.item_materials.contains_key("default"),
            "asset config must define `item_materials.default`"
        );
        // Every alias must resolve to a real material so a typo can't go
        // unnoticed until something tries to render at runtime.
        for (alias, target) in &self.aliases {
            anyhow::ensure!(
                self.materials.contains_key(target),
                "alias `{alias}` points to unknown material `{target}`"
            );
        }
        Ok(())
    }

    pub fn material_for_item(&self, item_type: ItemType) -> &MaterialDef {
        let name = match item_type {
            ItemType::SpeedPowerUp => "SpeedPowerUp",
            ItemType::MultiShotPowerUp => "MultiShotPowerUp",
            ItemType::PhasingPowerUp => "PhasingPowerUp",
            ItemType::Cookie => "Cookie",
        };
        let id = self
            .item_materials
            .get(name)
            .or_else(|| self.item_materials.get("default"))
            .expect("item_materials must define `default`");
        self.material(id)
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

    // Aliases let `map.json` reference textures by role (e.g. `natural-ground`)
    // instead of by the underlying material ID. Lookup falls through to the
    // input name when no alias is defined, so raw material IDs keep working.
    fn material(&self, id: &str) -> &MaterialDef {
        let resolved = self.aliases.get(id).map(String::as_str).unwrap_or(id);
        self.materials
            .get(resolved)
            .unwrap_or_else(|| panic!("asset set is missing material {id:?} (resolved to {resolved:?})"))
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
    // Horizontal model offset relative to the gameplay collider center.
    #[serde(default)]
    pub x_offset: f32,
    // Model bottom offset relative to the gameplay collider bottom.
    #[serde(default)]
    pub y_offset: f32,
    // Horizontal model offset relative to the gameplay collider center.
    #[serde(default)]
    pub z_offset: f32,
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

impl AssetSet {
    // Every disk path (`.png`, `.glb`, `.ogg`, …) referenced from `assets.json`.
    // GLTF subscene specifiers (`foo.glb#Scene0`) are split so callers only get
    // the file path — what the case-existence audit cares about on disk.
    #[cfg(test)]
    fn referenced_asset_paths(&self) -> Vec<String> {
        fn push(out: &mut Vec<String>, raw: &str) {
            let path = raw.split('#').next().unwrap_or(raw);
            if !path.is_empty() {
                out.push(path.to_string());
            }
        }

        let mut out: Vec<String> = Vec::new();
        for material in self.materials.values() {
            push(&mut out, &material.textures.base_color);
            push(&mut out, &material.textures.normal);
            push(&mut out, &material.textures.occlusion);
            push(&mut out, &material.textures.metallic_roughness);
        }
        push(&mut out, &self.skybox.image);
        push(&mut out, &self.models.wall_light.scene);
        push(&mut out, &self.player.model.scene);
        for sound in self.player.sounds.values() {
            push(&mut out, sound);
        }
        for actor in self.actors.values() {
            push(&mut out, &actor.model.scene);
            push(&mut out, &actor.explosion_effect.image);
            for sound in actor.sounds.values() {
                push(&mut out, sound);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Bevy's `AssetServer.load` ends in `std::fs::File::open`, which is
    // case-sensitive on Linux but case-insensitive on macOS APFS (default)
    // and Windows NTFS. A casing typo in `assets.json` slips past Mac dev
    // testing and 404s on a Linux client. Walk each referenced path's parent
    // directory and assert the exact filename is present — `Path::exists`
    // would be fooled by macOS's case-insensitive layer.
    #[test]
    fn referenced_assets_exist_case_exactly() {
        let assets = AssetSet::load_default().expect("load assets");
        let assets_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");

        let mut errors: Vec<String> = Vec::new();
        for path in assets.referenced_asset_paths() {
            let full = assets_root.join(&path);
            let parent = full.parent().expect("path has parent");
            let want = full.file_name().expect("path has filename");
            let entries: HashSet<_> = match fs::read_dir(parent) {
                Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.file_name())).collect(),
                Err(_) => HashSet::new(),
            };
            if entries.contains(want) {
                continue;
            }
            let ci_match = entries
                .iter()
                .find(|n| n.to_string_lossy().eq_ignore_ascii_case(&want.to_string_lossy()))
                .map(|n| n.to_string_lossy().into_owned());
            match ci_match {
                Some(other) => errors.push(format!(
                    "`{path}` referenced in assets.json — disk has `{}/{}` (case mismatch)",
                    parent.file_name().unwrap_or_default().to_string_lossy(),
                    other,
                )),
                None => errors.push(format!("`{path}` referenced in assets.json — not found on disk")),
            }
        }

        assert!(errors.is_empty(), "asset path mismatches:\n  {}", errors.join("\n  "));
    }
}
