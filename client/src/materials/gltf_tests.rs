use std::time::{Duration, Instant};

use bevy::{
    gltf::{Gltf, GltfMaterial, GltfPlugin},
    image::{CompressedImageFormatSupport, CompressedImageFormats, ImagePlugin, ImageSampler},
    mesh::MeshPlugin,
    pbr::PbrPlugin,
    prelude::*,
    shader::Shader,
    world_serialization::WorldSerializationPlugin,
};

#[test]
fn model_gltf_textures_reach_standard_materials_with_correct_colour_spaces() {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/client/assets.json")).expect("asset catalog invalid");
    let mut paths: Vec<_> = catalog["actors"]
        .as_object()
        .expect("actor catalog missing")
        .values()
        .map(|actor| {
            actor["model"]["scene"]
                .as_str()
                .expect("actor scene missing")
                .split('#')
                .next()
                .expect("actor scene path empty")
                .to_owned()
        })
        .collect();
    paths.push(
        catalog["player"]["model"]["scene"]
            .as_str()
            .expect("player scene missing")
            .split('#')
            .next()
            .expect("player path empty")
            .to_owned(),
    );
    paths.extend(
        catalog["wall_lights"]
            .as_object()
            .expect("wall light catalog missing")
            .values()
            .map(|light| light["scene"].as_str().expect("wall light scene missing").to_owned()),
    );
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
            ..default()
        },
        TransformPlugin,
        WorldSerializationPlugin,
        ImagePlugin::default(),
        MeshPlugin,
        AnimationPlugin,
        GltfPlugin::default(),
    ));
    app.init_asset::<Shader>();
    app.add_plugins(PbrPlugin::default());
    app.insert_resource(CompressedImageFormatSupport(CompressedImageFormats::NONE));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<Gltf>> = paths.iter().map(|path| server.load(path.clone())).collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    while handles.iter().any(|handle| !server.is_loaded_with_dependencies(handle)) {
        assert!(Instant::now() < deadline, "actor models failed to load");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
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
