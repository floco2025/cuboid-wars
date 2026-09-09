use bevy::{gltf::Gltf, prelude::*};
use common::protocol::{CarrierId, MapLayout, Position, WallLight};

use super::map_wall_light_emissive_system;
use crate::{
    config::AssetSet,
    test_assets::{headless_asset_app, preload_gltfs},
    test_fixtures::map_settings,
};

#[test]
fn unknown_wall_light_kind_fails_before_rendering() {
    let assets = AssetSet::load_default().expect("asset catalog invalid");
    let layout = MapLayout {
        wall_lights: vec![WallLight {
            kind: "missing-fixture".into(),
            pos: Position::default(),
            yaw: 0.0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let error = assets
        .validate_map_bindings(&map_settings(), &layout)
        .expect_err("unknown light kind accepted");
    assert!(error.to_string().contains("missing-fixture"));
}

#[test]
fn wall_light_models_keep_emitters_separate_from_their_housing() {
    let mut catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/client/assets.json")).expect("asset catalog invalid");
    for (index, light) in catalog["wall_lights"]
        .as_object_mut()
        .expect("wall lights missing")
        .values_mut()
        .enumerate()
    {
        light["emissive_luminance"] = serde_json::json!(3.0 + index as f32 * 4.0);
    }
    let assets: AssetSet = serde_json::from_value(catalog).expect("asset catalog invalid");
    let definitions: Vec<_> = assets.wall_light_models().cloned().collect();
    let mut app = headless_asset_app(|app| {
        app.insert_resource(assets);
        app.add_systems(Update, map_wall_light_emissive_system);
    });
    let paths: Vec<_> = definitions.iter().map(|light| light.scene.clone()).collect();
    let handles = preload_gltfs(&mut app, &paths);
    let server = app.world().resource::<AssetServer>().clone();
    let gltfs = app.world().resource::<Assets<Gltf>>();
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    for (handle, definition) in handles.iter().zip(&definitions) {
        let gltf = gltfs.get(handle).expect("wall light glTF missing");
        let [r, g, b] = definition.color;
        let expected = LinearRgba::rgb(r, g, b) * definition.emissive_luminance;
        let mut emitters = 0;
        let mut housing = 0;
        for index in 0..gltf.materials.len() {
            let handle: Handle<StandardMaterial> = server
                .get_handle(format!("{}#Material{index}/std", definition.scene))
                .expect("standard material handle missing");
            let material = materials.get(&handle).expect("wall light material missing");
            if material.emissive != LinearRgba::BLACK {
                assert_eq!(material.emissive, expected);
                emitters += 1;
            } else {
                housing += 1;
            }
        }
        assert!(
            emitters > 0 && housing > 0,
            "fixture lacks a lit diffuser or an unlit housing: {}",
            definition.scene
        );
    }
}
