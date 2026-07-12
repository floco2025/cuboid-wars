use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

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
    barrier_kind_colors: HashMap<String, String>,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
    models: GenericModels,
    // Named skyboxes; the map's `MapSettings.skybox` selects one. BTreeMap so
    // the unknown-name fallback (sorted-first entry) is deterministic.
    skyboxes: BTreeMap<String, SkyboxDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetSetFile {
    version: u32,
    materials: HashMap<String, MaterialDef>,
    #[serde(default)]
    aliases: HashMap<String, String>,
    item_materials: HashMap<String, String>,
    // Per-barrier-kind hex colors keyed by the kind id from gameplay.json.
    #[serde(default)]
    barrier_kind_colors: HashMap<String, String>,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
    models: GenericModels,
    skyboxes: BTreeMap<String, SkyboxDef>,
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
            barrier_kind_colors: file.barrier_kind_colors,
            player: file.player,
            actors: file.actors,
            models: file.models,
            skyboxes: file.skyboxes,
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
        anyhow::ensure!(
            !self.skyboxes.is_empty(),
            "asset config must define at least one entry in `skyboxes`"
        );
        // Every alias must resolve to a real material so a typo can't go
        // unnoticed until something tries to render at runtime.
        for (alias, target) in &self.aliases {
            anyhow::ensure!(
                self.materials.contains_key(target),
                "alias `{alias}` points to unknown material `{target}`"
            );
        }
        // A zero-dimension sprite sheet underflows `initial_frame` and divides
        // by zero in `set_mesh_uvs`; reject it at load instead of at first kill.
        for (kind, actor) in &self.actors {
            let effect = &actor.explosion_effect;
            anyhow::ensure!(
                effect.columns >= 1 && effect.rows >= 1,
                "actor `{kind}` explosion_effect must have columns >= 1 and rows >= 1 (got {}x{})",
                effect.columns,
                effect.rows
            );
        }
        Ok(())
    }

    pub fn material_for_item(&self, item_type: ItemType) -> &MaterialDef {
        let name = match item_type {
            ItemType::SpeedPowerUp => "SpeedPowerUp",
            ItemType::MultiShotPowerUp => "MultiShotPowerUp",
            ItemType::PhasingPowerUp => "PhasingPowerUp",
            ItemType::LowGravityPowerUp => "LowGravityPowerUp",
            ItemType::HealthPotion => "HealthPotion",
            ItemType::Cookie => "Cookie",
            // Keys reuse the per-color barrier materials directly; no entry
            // in the `item_materials` table is expected.
            ItemType::Key(_) => unreachable!("keys do not use AssetSet::material_for_item"),
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

    // Hex color string (e.g. "#5090ff") configured for the given barrier kind
    // id. `None` if the kind has no color entry — the caller (BarrierKindColors
    // builder) should treat this as a hard error.
    pub fn barrier_kind_color_hex(&self, id: &str) -> Option<&str> {
        self.barrier_kind_colors.get(id).map(String::as_str)
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

    pub fn skybox(&self, name: &str) -> Option<&SkyboxDef> {
        self.skyboxes.get(name)
    }

    // Deterministic stand-in when a map names a skybox this client doesn't
    // have: the sorted-first entry. `validate` guarantees non-emptiness.
    pub fn fallback_skybox(&self) -> (&str, &SkyboxDef) {
        let (name, def) = self
            .skyboxes
            .iter()
            .next()
            .expect("skyboxes is empty despite validate() requiring an entry");
        (name, def)
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

    // Aliases are the only legal way `map.json` references textures: a face
    // value must be an alias key (assets.json::aliases). Raw material ids are
    // rejected so the alias system can't drift silently. The editor enforces
    // the same rule in `validate_map`; this is the runtime backstop.
    fn material(&self, id: &str) -> &MaterialDef {
        let resolved = self.aliases.get(id).map(String::as_str).unwrap_or_else(|| {
            panic!(
                "material {id:?} is not an alias; only `aliases` entries are legal in map.json. \
                 Add an alias for it in assets.json or pick an existing alias."
            )
        });
        self.materials
            .get(resolved)
            .unwrap_or_else(|| panic!("alias {id:?} points to unknown material {resolved:?}"))
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
    // Rotation applied to the model around the X axis at spawn. Use 180 for
    // GLBs authored head-down (common with FBX-via-Sketchfab pipelines).
    #[serde(default)]
    pub x_rotation_degrees: f32,
    // Which clip in the GLB to play. Most models put their main loop at 0,
    // but multi-clip exports (e.g. Sketchfab character sets) need an explicit
    // index — robot_5's Walk is at 20.
    #[serde(default)]
    pub animation_index: usize,
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
    // Peak intensity of the effect's point light (lumens), faded out over
    // the lifetime.
    #[serde(default = "default_effect_light_intensity")]
    pub light_intensity: f32,
}

const fn default_effect_light_intensity() -> f32 {
    500_000.0
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
    // Seconds per full ambient sky turn; 0 (or absent) = static sky.
    #[serde(default)]
    pub rotation_period_secs: f32,
    // Sun rotation advances in discrete steps of this size so shadow maps
    // stay pixel-stable between steps; 0 (or absent) = continuous (shadow
    // edges shimmer while the sun creeps).
    #[serde(default)]
    pub sun_step_degrees: f32,
    #[serde(default)]
    pub sun_disc: SunDiscDef,
}

// The visible sun: a camera-following emissive sphere along the directional
// light's incoming direction, so it always sits where the shadows say.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct SunDiscDef {
    // Far enough that map geometry reads in front of it, inside the 1000 m
    // far plane. `radius: 0` disables the disc.
    pub distance: f32,
    pub radius: f32,
    // Emissive luminance (cd/m²) — must dwarf the skybox `brightness` so the
    // disc tonemaps to clipped white, and drives the bloom glare halo.
    pub luminance: f32,
}

impl Default for SunDiscDef {
    fn default() -> Self {
        Self {
            distance: 400.0,
            radius: 12.0,
            luminance: 5_000_000.0,
        }
    }
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
        for skybox in self.skyboxes.values() {
            push(&mut out, &skybox.image);
        }
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
            let parent = full.parent().expect("path has no parent");
            let want = full.file_name().expect("path has no filename");
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
