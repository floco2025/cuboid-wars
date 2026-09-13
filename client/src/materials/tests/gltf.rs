use crate::test_fixtures;
use bevy::{
    gltf::{Gltf, GltfMaterial},
    image::ImageSampler,
    prelude::*,
};

use crate::{
    config::gltf_path,
    test_assets::{headless_asset_app, preload_gltfs},
};

#[test]
fn model_gltf_textures_reach_standard_materials_with_correct_colour_spaces() {
    let catalog: serde_json::Value = serde_json::from_str(test_fixtures::ASSETS_JSON).expect("asset catalog invalid");
    let mut paths: Vec<_> = catalog["actors"]["kinds"]
        .as_object()
        .expect("actor catalog missing")
        .values()
        .map(|actor| gltf_path(actor["model"]["scene"].as_str().expect("actor scene missing")))
        .collect();
    paths.push(gltf_path(
        catalog["player"]["model"]["scene"]
            .as_str()
            .expect("player scene missing"),
    ));
    paths.extend(
        catalog["wall_lights"]
            .as_object()
            .expect("wall light catalog missing")
            .values()
            .map(|light| light["scene"].as_str().expect("wall light scene missing").to_owned()),
    );
    let mut app = headless_asset_app(|_| {});
    let handles = preload_gltfs(&mut app, &paths);
    let server = app.world().resource::<AssetServer>().clone();
    let gltfs = app.world().resource::<Assets<Gltf>>();
    let authored = app.world().resource::<Assets<GltfMaterial>>();
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let images = app.world().resource::<Assets<Image>>();
    for (path, handle) in paths.iter().zip(&handles) {
        let gltf = gltfs.get(handle).expect("actor glTF missing");
        for (index, source) in gltf.materials.iter().enumerate() {
            let source = authored.get(source).expect("authored material missing");
            let handle: Handle<StandardMaterial> = server
                .get_handle(format!("{path}#Material{index}/std"))
                .expect("standard material handle missing");
            let material = materials.get(&handle).expect("actor standard material missing");
            for (expected, actual, srgb) in [
                (&source.base_color_texture, &material.base_color_texture, true),
                (&source.normal_map_texture, &material.normal_map_texture, false),
                (
                    &source.metallic_roughness_texture,
                    &material.metallic_roughness_texture,
                    false,
                ),
                (&source.occlusion_texture, &material.occlusion_texture, false),
            ] {
                assert_eq!(expected, actual, "texture binding changed in {path}");
                if let Some(handle) = actual {
                    let image = images.get(handle).expect("embedded image missing");
                    assert_eq!(
                        image.texture_descriptor.format.is_srgb(),
                        srgb,
                        "wrong colour space in {path}"
                    );
                    assert!(image.data.as_ref().is_some_and(|data| !data.is_empty()));
                    assert!(
                        matches!(image.sampler, ImageSampler::Descriptor(_)),
                        "glTF sampler missing in {path}"
                    );
                }
            }
        }
    }
}
