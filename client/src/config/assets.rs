use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use common::protocol::ItemType;
use serde::Deserialize;

const REQUIRED_PLAYER_SOUNDS: &[&str] = &[
    "barrier_impact",
    "bump_player",
    "bump_wall",
    "collect_cookie",
    "collect_power_up",
    "dry_fire",
    "eraser",
    "explodes",
    "fall_damage",
    "fire",
    "hit_actor",
    "hit_player",
    "hit_wall",
    "laser_show",
    "missile_launch",
    "plate_press",
    "plate_release",
    "portal_fire",
    "quest_completed",
    "rain",
    "take_hit",
];
const REQUIRED_ACTOR_SOUNDS: &[&str] = &["explodes", "fire"];

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    materials: HashMap<String, MaterialDef>,
    ladder: MaterialBinding,
    pressure_plate: PressurePlateAssets,
    #[serde(default)]
    aliases: HashMap<String, String>,
    item_materials: HashMap<String, String>,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
    models: GenericModels,
    // Named skyboxes; the map's `MapSettings.skybox` selects one. BTreeMap so
    // the unknown-name fallback (sorted-first entry) is deterministic.
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
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
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
        for (path, binding) in [
            ("ladder", &self.ladder),
            ("pressure_plate.panel", &self.pressure_plate.panel),
            ("pressure_plate.frame", &self.pressure_plate.frame),
        ] {
            anyhow::ensure!(
                self.materials.contains_key(&binding.material),
                "`{path}.material` points to unknown material `{}`",
                binding.material
            );
        }
        for (name, id) in &self.item_materials {
            anyhow::ensure!(
                self.materials.contains_key(id),
                "`item_materials.{name}` points to unknown material `{id}`"
            );
        }
        validate_model("player.model", &self.player.model)?;
        for sound in REQUIRED_PLAYER_SOUNDS {
            validate_sound("player.sounds", &self.player.sounds, sound)?;
        }
        for (kind, actor) in &self.actors {
            validate_model(&format!("actors.{kind}.model"), &actor.model)?;
            for sound in REQUIRED_ACTOR_SOUNDS {
                validate_sound(&format!("actors.{kind}.sounds"), &actor.sounds, sound)?;
            }
        }
        Ok(())
    }

    pub fn validate_gameplay_bindings<'a>(&self, actor_kinds: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let gameplay_kinds = actor_kinds.into_iter().collect::<HashSet<_>>();
        let asset_kinds = self.actors.keys().map(String::as_str).collect::<HashSet<_>>();
        if gameplay_kinds != asset_kinds {
            let mut only_gameplay = gameplay_kinds.difference(&asset_kinds).copied().collect::<Vec<_>>();
            let mut only_assets = asset_kinds.difference(&gameplay_kinds).copied().collect::<Vec<_>>();
            only_gameplay.sort_unstable();
            only_assets.sort_unstable();
            bail!(
                "actor kinds disagree between server gameplay and client asset configs (only in gameplay: {only_gameplay:?}, only in assets: {only_assets:?})"
            );
        }
        Ok(())
    }

    #[must_use]
    pub fn ladder_material_id(&self) -> &str {
        &self.ladder.material
    }

    #[must_use]
    pub fn ladder_material_def(&self) -> &MaterialDef {
        self.exact_material(&self.ladder.material)
    }

    #[must_use]
    pub fn plate_panel_material_def(&self) -> &MaterialDef {
        self.exact_material(&self.pressure_plate.panel.material)
    }

    #[must_use]
    pub fn plate_frame_material_def(&self) -> &MaterialDef {
        self.exact_material(&self.pressure_plate.frame.material)
    }

    pub fn material_for_item(&self, item_type: ItemType) -> &MaterialDef {
        let name = match item_type {
            ItemType::SpeedPowerUp => "SpeedPowerUp",
            ItemType::MultiShotPowerUp => "MultiShotPowerUp",
            ItemType::LowGravityPowerUp => "LowGravityPowerUp",
            ItemType::PortalGunPowerUp => "PortalGunPowerUp",
            ItemType::HealthPotion => "HealthPotion",
            ItemType::Cookie => "Cookie",
            ItemType::MissilePack => "MissilePack",
            // Keys reuse the per-color barrier materials directly; no entry
            // in the `item_materials` table is expected.
            ItemType::Key(_) => unreachable!("keys do not use AssetSet::material_for_item"),
        };
        let id = self
            .item_materials
            .get(name)
            .or_else(|| self.item_materials.get("default"))
            .expect("`default` missing from item_materials");
        self.exact_material(id)
    }

    // Direct lookup for references INSIDE assets.json (item materials, the
    // ladder). Aliases are the map-authoring vocabulary — indirection only
    // earns its keep for references living outside this file, so internal
    // bindings name concrete materials.
    fn exact_material(&self, id: &str) -> &MaterialDef {
        self.materials
            .get(id)
            .unwrap_or_else(|| panic!("material {id:?} missing from `materials`"))
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

fn validate_model(path: &str, model: &ModelDef) -> Result<()> {
    anyhow::ensure!(!model.scene.trim().is_empty(), "`{path}.scene` must not be empty");
    anyhow::ensure!(
        model.scale.is_finite() && model.scale > 0.0,
        "`{path}.scale` must be positive and finite, got {}",
        model.scale
    );
    for (field, value) in [
        ("x_offset", model.x_offset),
        ("y_offset", model.y_offset),
        ("z_offset", model.z_offset),
        ("x_rotation_degrees", model.x_rotation_degrees),
    ] {
        anyhow::ensure!(value.is_finite(), "`{path}.{field}` must be finite, got {value}");
    }
    if let Some(speed) = model.animation_speed {
        anyhow::ensure!(
            speed.is_finite() && speed > 0.0,
            "`{path}.animation_speed` must be positive and finite, got {speed}"
        );
    }
    Ok(())
}

fn validate_sound(path: &str, sounds: &HashMap<String, String>, name: &str) -> Result<()> {
    let Some(asset_path) = sounds.get(name) else {
        bail!("asset config is missing required `{path}.{name}`");
    };
    anyhow::ensure!(!asset_path.trim().is_empty(), "`{path}.{name}` must not be empty");
    Ok(())
}

// A fixture's material (`ladder`, the `pressure_plate` parts): one entry of
// `materials`.
#[derive(Debug, Clone, Deserialize)]
struct MaterialBinding {
    material: String,
}

// The plate housing: the walkway panel and the frame around it.
#[derive(Debug, Clone, Deserialize)]
struct PressurePlateAssets {
    panel: MaterialBinding,
    frame: MaterialBinding,
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

    // The server's actor kinds, straight from the shipped JSON.
    fn server_actor_kinds() -> Vec<String> {
        let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay config does not parse");
        gameplay["actors"]["kinds"]
            .as_object()
            .expect("actors.kinds is not an object")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn default_assets_match_server_gameplay() {
        let assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        let kinds = server_actor_kinds();

        assets
            .validate_gameplay_bindings(kinds.iter().map(String::as_str))
            .expect("client asset bindings fail for the shipped gameplay");
    }

    #[test]
    fn actor_kind_set_mismatch_is_rejected() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        let kinds = server_actor_kinds();
        assets.actors.remove("mine");

        let error = assets
            .validate_gameplay_bindings(kinds.iter().map(String::as_str))
            .expect_err("missing actor assets must fail");

        assert!(error.to_string().contains("only in gameplay: [\"mine\"]"));
    }

    #[test]
    fn missing_required_actor_sound_is_rejected() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        assets
            .actors
            .get_mut("mine")
            .expect("mine actor missing from assets")
            .sounds
            .remove("explodes");

        let error = assets.validate().expect_err("missing sound must fail");

        assert!(error.to_string().contains("actors.mine.sounds.explodes"));
    }

    #[test]
    fn invalid_actor_model_is_rejected() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        assets
            .actors
            .get_mut("mine")
            .expect("mine actor missing from assets")
            .model
            .scale = 0.0;

        let error = assets.validate().expect_err("invalid model must fail");

        assert!(error.to_string().contains("actors.mine.model.scale"));
    }

    // Bevy's `AssetServer.load` ends in `std::fs::File::open`, which is
    // case-sensitive on Linux but case-insensitive on macOS APFS (default)
    // and Windows NTFS. A casing typo in `assets.json` slips past Mac dev
    // testing and 404s on a Linux client. Walk each referenced path's parent
    // directory and assert the exact filename is present — `Path::exists`
    // would be fooled by macOS's case-insensitive layer.
    #[test]
    fn referenced_assets_exist_case_exactly() {
        let assets = AssetSet::load_default().expect("shipped assets.json fails to load");
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
