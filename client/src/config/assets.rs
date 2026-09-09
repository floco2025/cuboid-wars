use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::{Component, Resource};
use common::protocol::{MapLayout, MapSettings, validate_texture_catalog, validate_texture_materials};
use serde::Deserialize;

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
    materials: HashMap<String, MaterialDef>,
    ladder: MaterialBinding,
    pressure_plate: PressurePlateAssets,
    #[serde(default)]
    aliases: HashMap<String, String>,
    player: PlayerAssets,
    actors: HashMap<String, ActorAssets>,
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

    fn validate(&self) -> Result<()> {
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

    pub fn material_by_id(&self, id: &str) -> &MaterialDef {
        self.material(id)
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
        model.scene_index().is_some(),
        "`{path}.scene` must reference a `#Scene<n>` label, got {}",
        model.scene
    );
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
    if let Some(wheels) = model.wheels {
        anyhow::ensure!(
            model.animation_speed.is_none(),
            "`{path}` cannot combine wheels with animation_speed"
        );
        for (field, value) in [
            ("radius", wheels.radius),
            ("track", wheels.track),
            ("wheelbase", wheels.wheelbase),
            ("drive_cycle_secs", wheels.drive_cycle_secs),
        ] {
            anyhow::ensure!(
                value.is_finite() && value > 0.0,
                "`{path}.wheels.{field}` must be positive and finite"
            );
        }
        anyhow::ensure!(
            wheels.idle_animation != wheels.drive_animation,
            "`{path}.wheels` needs distinct idle_animation and drive_animation clips"
        );
    }
    if let Some(rig) = &model.aim_rig {
        for (field, name) in [
            ("yaw_node", &rig.yaw_node),
            ("pitch_node", &rig.pitch_node),
            ("muzzle_node", &rig.muzzle_node),
        ] {
            anyhow::ensure!(!name.trim().is_empty(), "`{path}.aim_rig.{field}` must not be empty");
        }
        anyhow::ensure!(
            rig.yaw_node != rig.pitch_node && rig.yaw_node != rig.muzzle_node && rig.pitch_node != rig.muzzle_node,
            "`{path}.aim_rig` needs distinct yaw, pitch, and muzzle nodes"
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
    pub x_offset: f32,
    // Model origin offset from the character's feet.
    #[serde(default)]
    pub y_offset: f32,
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
    #[serde(default)]
    pub wheels: Option<WheelModelDef>,
    #[serde(default)]
    pub aim_rig: Option<AimRigDef>,
    #[serde(default = "default_true")]
    pub rotate_with_facing: bool,
}

fn default_true() -> bool {
    true
}

impl ModelDef {
    // The index of the `#Scene<n>` label in the scene reference.
    #[must_use]
    pub fn scene_index(&self) -> Option<usize> {
        self.scene
            .split_once('#')
            .and_then(|(_, label)| label.strip_prefix("Scene"))
            .and_then(|index| index.parse().ok())
    }
}

// The GLB path of a `path#Scene<n>` scene reference.
#[must_use]
pub fn gltf_path(scene: &str) -> String {
    scene.split('#').next().unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, Deserialize, Component)]
#[serde(deny_unknown_fields)]
pub struct AimRigDef {
    pub yaw_node: String,
    pub pitch_node: String,
    pub muzzle_node: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelModelDef {
    pub radius: f32,
    pub track: f32,
    pub wheelbase: f32,
    pub idle_animation: usize,
    pub drive_animation: usize,
    pub drive_cycle_secs: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub color: [f32; 3],
    pub flicker: bool,
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
    pub sun_disc: SunDiscDef,
}

// The visible sun: a camera-following emissive sphere along the directional
// light's incoming direction, so it always sits where the shadows say.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SunDiscDef {
    // Far enough that map geometry reads in front of it, inside the 1000 m
    // far plane. `radius: 0` disables the disc.
    pub distance: f32,
    pub radius: f32,
    // Emissive luminance (cd/m²) — must dwarf the skybox `brightness` so the
    // disc tonemaps to clipped white, and drives the bloom glare halo.
    pub luminance: f32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::TextureSettings;
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
    fn missing_map_texture_binding_fails_before_rendering() {
        let assets = AssetSet::load_default().expect("shipped assets are invalid");
        let mut settings = crate::test_geometry::map_settings();
        settings
            .textures
            .insert("missing-binding".to_owned(), TextureSettings { portalable: false });
        let error = assets
            .validate_map_bindings(&settings, &MapLayout::default())
            .expect_err("missing binding was accepted");
        assert!(error.to_string().contains("missing-binding"));
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
        assets.actors.remove("scuttler");

        let error = assets
            .validate_gameplay_bindings(kinds.iter().map(String::as_str))
            .expect_err("missing actor assets must fail");

        assert!(error.to_string().contains("only in gameplay: [\"scuttler\"]"));
    }

    #[test]
    fn actor_catalog_can_be_replaced_with_arbitrary_names() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        let definitions: Vec<_> = assets.actors.values().cloned().collect();
        assets.actors.clear();
        for (index, actor) in definitions.into_iter().enumerate() {
            assets.actors.insert(format!("custom_actor_{index}"), actor);
        }
        assets.validate().expect("renamed actor assets rejected");
        assets
            .validate_gameplay_bindings(assets.actors.keys().map(String::as_str))
            .expect("matching custom actor catalog rejected");
        assets.actors.clear();
        assets.validate().expect("empty actor catalog rejected");
        assets
            .validate_gameplay_bindings([])
            .expect("matching empty actor catalog rejected");
    }

    #[test]
    fn malformed_model_capabilities_are_rejected() {
        let mut model: ModelDef = serde_json::from_value(serde_json::json!({
            "scene": "models/custom.glb#Scene0", "scale": 1.0,
            "wheels": { "radius": 0.3, "track": 0.8, "wheelbase": 0.7,
                "idle_animation": 3, "drive_animation": 7, "drive_cycle_secs": 2.0 },
            "aim_rig": { "yaw_node": "Pan", "pitch_node": "Elevation", "muzzle_node": "Emitter" }
        }))
        .expect("custom model config rejected");
        validate_model("custom.model", &model).expect("valid model capabilities rejected");
        model.wheels.as_mut().expect("wheel config missing").radius = 0.0;
        assert!(
            validate_model("custom.model", &model)
                .expect_err("zero radius accepted")
                .to_string()
                .contains("wheels.radius")
        );
        model.wheels = None;
        model
            .aim_rig
            .as_mut()
            .expect("aim rig config missing")
            .muzzle_node
            .clear();
        assert!(
            validate_model("custom.model", &model)
                .expect_err("empty node accepted")
                .to_string()
                .contains("aim_rig.muzzle_node")
        );
    }

    #[test]
    fn missing_required_actor_sound_is_rejected() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        assets
            .actors
            .get_mut("scuttler")
            .expect("scuttler actor missing from assets")
            .sounds
            .remove("explodes");

        let error = assets.validate().expect_err("missing sound must fail");

        assert!(error.to_string().contains("actors.scuttler.sounds.explodes"));
    }

    #[test]
    fn invalid_actor_model_is_rejected() {
        let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
        assets
            .actors
            .get_mut("scuttler")
            .expect("scuttler actor missing from assets")
            .model
            .scale = 0.0;

        let error = assets.validate().expect_err("invalid model must fail");

        assert!(error.to_string().contains("actors.scuttler.model.scale"));
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
