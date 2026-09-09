use std::{collections::HashSet, fs, path::Path};

use common::protocol::{MapLayout, TextureSettings};

use super::{AssetSet, ModelDef, model::validate_model};
use crate::test_fixtures::map_settings;

// The server's actor kinds, straight from the shipped JSON.
fn server_actor_kinds() -> Vec<String> {
    let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../../config/server/gameplay.json"))
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
    let mut settings = map_settings();
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

#[test]
fn normal_map_without_a_convention_suffix_is_rejected() {
    let mut assets = AssetSet::load_default().expect("shipped assets.json fails to load");
    let (name, material) = assets
        .materials
        .iter_mut()
        .next()
        .expect("shipped assets.json has no materials");
    let name = name.clone();
    material.textures.normal = "textures/example/example-normal.png".to_owned();

    let error = assets.validate().expect_err("unsuffixed normal map must fail");

    assert!(error.to_string().contains(&format!("materials.{name}.textures.normal")));
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
