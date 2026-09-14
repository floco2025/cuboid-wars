use super::super::{
    fixtures,
    material::GrassMaterials,
    streaming::{GrassChunkBuild, grass_streaming_system},
};
use super::*;
use crate::{cameras::MainCameraMarker, config::ClientSettings, map::DebugColorMode};
use common::map::GroundsSettings;

fn app() -> App {
    let mut app = fixtures::app();
    app.world_mut().resource_mut::<ClientSettings>().grass.enabled = true;
    app.init_resource::<GrassSources>()
        .init_resource::<GrassChunks>()
        .init_resource::<DebugColors>()
        .init_resource::<Carriers>()
        .insert_resource(MapLayout {
            grounds: Some(Grounds {
                half_size: [10.0, 10.0],
                y: 0.0,
                settings: GroundsSettings { level: 0 },
            }),
            ..default()
        })
        .add_systems(
            Update,
            (grass_sources_reset_system, grass_streaming_system)
                .chain()
                .after(super::super::material::setup_grass_materials_system),
        );
    app.world_mut().spawn((
        MainCameraMarker,
        GlobalTransform::from_translation(Vec3::new(25.0, 2.0, 0.0)),
    ));
    app.update();
    app
}

#[test]
fn debug_color_cycle_preserves_exterior_chunks_and_materials() {
    let mut app = app();
    let chunks: Vec<_> = app
        .world()
        .resource::<GrassChunks>()
        .spawned
        .values()
        .copied()
        .collect();
    assert!(!chunks.is_empty());
    let materials = app.world().resource::<GrassMaterials>();
    let original = (materials.grass.id(), materials.terrain.id());
    for mode in [
        DebugColorMode::ByMaterial,
        DebugColorMode::BySegment,
        DebugColorMode::Off,
    ] {
        app.world_mut().resource_mut::<DebugColors>().0 = mode;
        app.update();
        assert!(app.world().resource::<GrassSources>().grounds.is_some());
        assert!(chunks.iter().all(|entity| app.world().get_entity(*entity).is_ok()));
        let materials = app.world().resource::<GrassMaterials>();
        assert_eq!((materials.grass.id(), materials.terrain.id()), original);
    }
}

#[test]
fn layout_replacement_discards_outstanding_builds_and_sources() {
    let mut app = app();
    let old: Vec<_> = app
        .world()
        .resource::<GrassChunks>()
        .spawned
        .values()
        .copied()
        .collect();
    assert!(
        old.iter()
            .all(|entity| app.world().get::<GrassChunkBuild>(*entity).is_some())
    );
    *app.world_mut().resource_mut::<MapLayout>() = MapLayout::default();
    app.update();
    assert!(old.iter().all(|entity| app.world().get_entity(*entity).is_err()));
    assert!(app.world().resource::<GrassChunks>().spawned.is_empty());
    assert!(app.world().resource::<GrassSources>().grounds.is_none());
}
