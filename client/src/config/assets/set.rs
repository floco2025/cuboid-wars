use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use common::protocol::{MapLayout, MapSettings, validate_texture_catalog, validate_texture_materials};
use serde::Deserialize;

use super::{
    lighting::{SkyboxDef, WallLightModelDef},
    material::{MaterialBinding, MaterialDef, PressurePlateAssets},
    model::{ModelDef, validate_model},
};

const REQUIRED_PLAYER_SOUNDS: &[&str] = &[
    "barrier_impact",
    "bump_player",
    "bump_wall",
    "collect_gold",
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
    "portal_fizzle",
    "quest_completed",
    "rain",
    "take_hit",
    "void_fall",
];
const REQUIRED_ACTOR_SOUNDS: &[&str] = &["explodes", "fire"];

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    pub(super) materials: HashMap<String, MaterialDef>,
    ladder: MaterialBinding,
    pressure_plate: PressurePlateAssets,
    #[serde(default)]
    aliases: HashMap<String, String>,
    player: PlayerAssets,
    pub(super) actors: HashMap<String, ActorAssets>,
    wall_lights: HashMap<String, WallLightModelDef>,
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

    pub(super) fn validate(&self) -> Result<()> {
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
        for (name, material) in &self.materials {
            anyhow::ensure!(
                material.textures.normal_is_directx().is_some(),
                "`materials.{name}.textures.normal` must be named `-normal-dx` or `-normal-gl`, got `{}`",
                material.textures.normal
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
        validate_model("player.model", &self.player.model)?;
        for sound in REQUIRED_PLAYER_SOUNDS {
            validate_sound("player.sounds", &self.player.sounds, sound)?;
        }
        for (kind, light) in &self.wall_lights {
            anyhow::ensure!(
                !kind.trim().is_empty() && !light.scene.trim().is_empty(),
                "wall_lights keys and scene paths must not be empty"
            );
            for (field, value) in [("scale", light.scale), ("range", light.range)] {
                anyhow::ensure!(
                    value.is_finite() && value > 0.0,
                    "wall_lights.{kind}.{field} must be positive"
                );
            }
            for (field, value) in [
                ("brightness", light.brightness),
                ("radius", light.radius),
                ("offset_from_wall", light.offset_from_wall),
                ("emissive_luminance", light.emissive_luminance),
            ] {
                anyhow::ensure!(
                    value.is_finite() && value >= 0.0,
                    "wall_lights.{kind}.{field} must be nonnegative"
                );
            }
            anyhow::ensure!(
                light.color.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "wall_lights.{kind}.color must contain RGB values from 0 to 1"
            );
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

    pub fn validate_map_bindings(&self, settings: &MapSettings, layout: &MapLayout) -> Result<()> {
        for light in &layout.wall_lights {
            anyhow::ensure!(
                self.wall_lights.contains_key(&light.kind),
                "map wall light kind {:?} has no binding in assets.json",
                light.kind
            );
        }
        validate_texture_catalog(&settings.textures, "map.textures")?;
        for alias in settings.textures.keys() {
            anyhow::ensure!(
                self.aliases.contains_key(alias),
                "map texture alias {alias:?} has no binding in assets.json"
            );
        }
        for (kind, materials) in [
            ("walls", &layout.wall_materials),
            ("floors", &layout.floor_materials),
            ("ramps", &layout.ramp_materials),
        ] {
            for (index, faces) in materials.iter().enumerate() {
                validate_texture_materials(faces, &settings.textures, &format!("map.{kind}[{index}]"))?;
            }
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

    // Direct lookup for references INSIDE assets.json (ladders and plates). Aliases are the map-authoring vocabulary — indirection only
    // earns its keep for references living outside this file, so internal
    // bindings name concrete materials.
    fn exact_material(&self, id: &str) -> &MaterialDef {
        self.materials
            .get(id)
            .unwrap_or_else(|| panic!("material {id:?} missing from `materials`"))
    }

    pub fn player_model(&self) -> &ModelDef {
        &self.player.model
    }

    pub fn actor_model(&self, kind: &str) -> &ModelDef {
        &self.actor(kind).model
    }

    pub fn wall_light_model(&self, kind: &str) -> &WallLightModelDef {
        self.wall_lights
            .get(kind)
            .expect("wall light kind missing from assets.json")
    }

    pub fn wall_light_models(&self) -> impl Iterator<Item = &WallLightModelDef> {
        self.wall_lights.values()
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
    pub fn material_by_id(&self, id: &str) -> &MaterialDef {
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

fn validate_sound(path: &str, sounds: &HashMap<String, String>, name: &str) -> Result<()> {
    let Some(asset_path) = sounds.get(name) else {
        bail!("asset config is missing required `{path}.{name}`");
    };
    anyhow::ensure!(!asset_path.trim().is_empty(), "`{path}.{name}` must not be empty");
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct PlayerAssets {
    model: ModelDef,
    sounds: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ActorAssets {
    pub(super) model: ModelDef,
    pub(super) sounds: HashMap<String, String>,
}

impl AssetSet {
    // Every disk path (`.png`, `.glb`, `.ogg`, …) referenced from `assets.json`.
    // GLTF subscene specifiers (`foo.glb#Scene0`) are split so callers only get
    // the file path — what the case-existence audit cares about on disk.
    #[cfg(test)]
    pub(super) fn referenced_asset_paths(&self) -> Vec<String> {
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
        for light in self.wall_lights.values() {
            push(&mut out, &light.scene);
        }
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
