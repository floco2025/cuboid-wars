// The headless Bevy app that asset tests load shipped models into, and the
// waits that make a spawned scene safe to sample.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use bevy::{
    gltf::{Gltf, GltfPlugin},
    image::{CompressedImageFormatSupport, CompressedImageFormats, ImagePlugin},
    mesh::MeshPlugin,
    pbr::PbrPlugin,
    prelude::*,
    shader::Shader,
    time::TimeUpdateStrategy,
    world_serialization::{WorldInstanceReady, WorldSerializationPlugin},
};
use common::constants::TICK_SECS;

use crate::characters::character_models_attach_system;

const LOAD_DEADLINE: Duration = Duration::from_secs(30);
const SETTLED_FRAMES: usize = 6;

// How many times each scene root has become ready; a root that is ready
// twice was respawned, and whatever the test set up on the first instance
// is gone.
#[derive(Resource, Default)]
struct InstanceReadyCounts(HashMap<Entity, usize>);

// Every plugin a shipped GLB needs to become entities, standard materials,
// and playing animations without a window or GPU. `configure` adds the
// systems and resources under test before the app is finished.
pub(crate) fn headless_asset_app(configure: impl FnOnce(&mut App)) -> App {
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
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(TICK_SECS)));
    app.init_resource::<InstanceReadyCounts>();
    app.add_systems(Update, character_models_attach_system);
    app.add_observer(
        |ready: On<WorldInstanceReady>, mut counts: ResMut<InstanceReadyCounts>| {
            *counts.0.entry(ready.entity).or_default() += 1;
        },
    );
    configure(&mut app);
    app.finish();
    app.cleanup();
    app
}

// Load the GLBs at `paths` to completion before anything asks for their
// labelled sub-assets: the asset server loads the whole file again for every
// label requested while it is still in flight, and each extra completion
// re-inserts the sub-assets and respawns every instance of the scene.
pub(crate) fn preload_gltfs(app: &mut App, paths: &[String]) -> Vec<Handle<Gltf>> {
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<Gltf>> = paths.iter().map(|path| server.load(path.clone())).collect();
    let deadline = Instant::now() + LOAD_DEADLINE;
    while !handles.iter().all(|handle| server.is_loaded_with_dependencies(handle)) {
        assert!(Instant::now() < deadline, "GLB failed to load: {paths:?}");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    handles
}

// Update until `ready` holds and the world has stopped gaining or losing
// entities, then check that no scene root became ready more than once.
pub(crate) fn settle(app: &mut App, mut ready: impl FnMut(&mut World) -> bool) {
    let deadline = Instant::now() + LOAD_DEADLINE;
    while !ready(app.world_mut()) {
        assert!(Instant::now() < deadline, "scene never became ready");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut flat_frames = 0;
    while flat_frames < SETTLED_FRAMES {
        let before = app.world().entities().len();
        app.update();
        flat_frames = if app.world().entities().len() == before {
            flat_frames + 1
        } else {
            0
        };
        assert!(Instant::now() < deadline, "scene never settled");
    }
    for (root, count) in &app.world().resource::<InstanceReadyCounts>().0 {
        assert_eq!(*count, 1, "scene root {root:?} was spawned {count} times");
    }
}
