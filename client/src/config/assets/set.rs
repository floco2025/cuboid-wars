use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use common::protocol::{
    MapLayout, MapSettings, TERRAIN_MATERIAL, TextureSettings, validate_texture_catalog, validate_texture_materials,
};
use serde::Deserialize;

use super::{
    FootstepSounds,
    lighting::WallLightModelDef,
    material::{MaterialBinding, MaterialDef},
    model::{ModelDef, validate_model},
    pressure_plate::PressurePlateDef,
    sound::{SoundDef, validate_volume},
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
    "landing",
    "laser_show",
    "missile_launch",
    "plate_press",
    "plate_release",
    "portal_fire",
    "portal_fizzle",
    "quest_completed",
    "checkpoint_reached",
    "rain",
    "take_hit",
    "void_fall",
];
const REQUIRED_ACTOR_SOUNDS: &[&str] = &["explodes"];

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct AssetSet {
    pub footsteps: FootstepSounds,
    pub(super) materials: HashMap<String, MaterialDef>,
    ladder: MaterialBinding,
    rocks: MaterialBinding,
    terrain: MaterialBinding,
    pressure_plate: PressurePlateDef,
    player: PlayerAssets,
    pub actors: ActorCatalog,
    wall_lights: HashMap<String, WallLightModelDef>,
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
        self.footsteps.validate()?;
        validate_volume(self.actors.movement_volume_db, "actors.movement_volume_db")?;
        validate_volume(self.actors.sfx_volume_db, "actors.sfx_volume_db")?;
        for (name, material) in &self.materials {
            if let Some(binding) = &material.footstep {
                self.footsteps
                    .validate_binding(binding, &format!("materials.{name}.footstep"))?;
            }
            if let Some(textures) = &material.textures {
                anyhow::ensure!(
                    textures.normal_is_directx().is_some(),
                    "`materials.{name}.textures.normal` must be named `-normal-dx` or `-normal-gl`, got `{}`",
                    textures.normal
                );
            }
        }
        anyhow::ensure!(
            self.materials.contains_key(&self.ladder.material),
            "`ladder.material` points to unknown material `{}`",
            self.ladder.material
        );
        anyhow::ensure!(
            self.materials.contains_key(&self.rocks.material),
            "`rocks.material` points to unknown material `{}`",
            self.rocks.material
        );
        anyhow::ensure!(
            self.materials.contains_key(&self.terrain.material),
            "`terrain.material` points to unknown material `{}`",
            self.terrain.material
        );
        self.pressure_plate.validate()?;
        validate_model("player.model", &self.player.model)?;
        validate_sounds("player.sounds", &self.player.sounds, REQUIRED_PLAYER_SOUNDS)?;
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
        for (kind, actor) in &self.actors.kinds {
            validate_model(&format!("actors.kinds.{kind}.model"), &actor.model)?;
            validate_sounds(
                &format!("actors.kinds.{kind}.sounds"),
                &actor.sounds,
                REQUIRED_ACTOR_SOUNDS,
            )?;
        }
        Ok(())
    }

    pub fn validate_gameplay_bindings<'a>(&self, actor_kinds: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let gameplay_kinds = actor_kinds.into_iter().collect::<HashSet<_>>();
        let asset_kinds = self.actors.kinds.keys().map(String::as_str).collect::<HashSet<_>>();
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
        for (alias, texture) in &settings.textures {
            anyhow::ensure!(
                self.materials.contains_key(&texture.material),
                "map texture alias {alias:?} names material {:?}, which assets.json does not define",
                texture.material
            );
        }
        for (kind, materials) in [
            ("walls", &layout.wall_materials),
            ("floors", &layout.floor_materials),
            ("ramps", &layout.ramp_materials),
        ] {
            for (index, faces) in materials.iter().enumerate() {
                if kind == "floors" && faces.top == TERRAIN_MATERIAL {
                    let mut authored_faces = faces.clone();
                    authored_faces.top = authored_faces.bottom.clone();
                    validate_texture_materials(&authored_faces, &settings.textures, &format!("map.{kind}[{index}]"))?;
                } else {
                    validate_texture_materials(faces, &settings.textures, &format!("map.{kind}[{index}]"))?;
                }
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
    pub fn rock_material_def(&self) -> &MaterialDef {
        self.exact_material(&self.rocks.material)
    }

    #[must_use]
    pub fn terrain_material_def(&self) -> &MaterialDef {
        self.exact_material(&self.terrain.material)
    }

    #[must_use]
    pub fn pressure_plate(&self) -> &PressurePlateDef {
        &self.pressure_plate
    }

    #[must_use]
    pub fn map_materials<'a>(&'a self, textures: &'a BTreeMap<String, TextureSettings>) -> MapMaterials<'a> {
        MapMaterials { assets: self, textures }
    }

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

    pub fn player_sound(&self, name: &str) -> &SoundDef {
        self.player
            .sounds
            .get(name)
            .unwrap_or_else(|| panic!("asset set is missing player sound {name:?}"))
    }

    pub fn actor_sound(&self, kind: &str, name: &str) -> Option<&SoundDef> {
        self.actor(kind).sounds.get(name)
    }

    fn actor(&self, kind: &str) -> &ActorAssets {
        self.actors
            .kinds
            .get(kind)
            .unwrap_or_else(|| panic!("asset set is missing actor kind {kind:?}"))
    }
}

// A map's face aliases resolved through its textures catalog, which `validate_map_bindings` has checked.
#[derive(Clone, Copy)]
pub struct MapMaterials<'a> {
    assets: &'a AssetSet,
    textures: &'a BTreeMap<String, TextureSettings>,
}

impl<'a> MapMaterials<'a> {
    #[must_use]
    pub fn get(self, alias: &str) -> &'a MaterialDef {
        if alias == TERRAIN_MATERIAL {
            return self.assets.terrain_material_def();
        }
        let texture = self
            .textures
            .get(alias)
            .unwrap_or_else(|| panic!("texture alias {alias:?} missing from the map's textures"));
        self.assets.exact_material(&texture.material)
    }
}

fn validate_sounds(path: &str, sounds: &HashMap<String, SoundDef>, required: &[&str]) -> Result<()> {
    for name in required {
        if !sounds.contains_key(*name) {
            bail!("asset config is missing required `{path}.{name}`");
        }
    }
    for (name, sound) in sounds {
        sound.validate(&format!("{path}.{name}"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct PlayerAssets {
    model: ModelDef,
    sounds: HashMap<String, SoundDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorCatalog {
    pub movement_volume_db: f32,
    pub sfx_volume_db: f32,
    pub(super) kinds: HashMap<String, ActorAssets>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ActorAssets {
    pub(super) model: ModelDef,
    pub(super) sounds: HashMap<String, SoundDef>,
}
