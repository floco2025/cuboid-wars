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
            grounds: Some(Grounds::new(
                [(-10.0, 10.0, -10.0, 10.0)],
                0.0,
                GroundsSettings { level: 0 },
            )),
            ..default()
        })
        .add_systems(Update, (grass_sources_reset_system, grass_streaming_system).chain());
    app.world_mut().spawn((
        MainCameraMarker,
        GlobalTransform::from_translation(Vec3::new(25.0, 2.0, 0.0)),
    ));
    app.update();
    app
}

#[test]
fn meadow_chunks_follow_an_offset_base_instead_of_the_grid_origin() {
    let meadow = GroundsGrass {
        grounds: Grounds::new([(93.0, 117.0, -97.0, -73.0)], 0.0, GroundsSettings { level: 0 }),
        level: 0,
    };
    assert!(!meadow.cell_is_meadow(IVec2::new(10, -9)), "inside the cutout");
    assert!(meadow.cell_is_meadow(IVec2::new(11, -9)), "partly outside the cutout");
    assert!(meadow.cell_is_meadow(IVec2::ZERO), "the old origin is now meadow");
    assert!(!meadow.cell_is_meadow(IVec2::new(200, -9)), "past the terrain edge");
}

#[test]
fn meadow_chunks_fill_a_concavity_and_the_gap_between_separate_bases() {
    let meadow = GroundsGrass {
        grounds: Grounds::new(
            [
                (-40.0, 50.0, -40.0, -20.0),
                (20.0, 50.0, -20.0, 40.0),
                (60.0, 80.0, -40.0, 40.0),
            ],
            0.0,
            GroundsSettings { level: 0 },
        ),
        level: 0,
    };
    assert!(meadow.cell_is_meadow(IVec2::ZERO), "inside the concavity");
    assert!(meadow.cell_is_meadow(IVec2::new(5, 0)), "between the bases");
    assert!(
        !meadow.cell_is_meadow(IVec2::new(3, 0)),
        "wholly inside the authored base"
    );
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
