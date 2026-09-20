use super::*;
use crate::map::grass::{fixtures, mesh::GrassLod, patch::GrassPatch, sources::GrassChunkSource};
use common::protocol::{CarrierId, Floor};

fn visual() -> GrassChunkVisual {
    GrassChunkVisual {
        revision: 0,
        burns: Vec::new(),
        clearance: default(),
        patches: vec![GrassPatch {
            x1: 0.0,
            x2: 10.0,
            z1: 0.0,
            z2: 10.0,
            y: 0.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        source: GrassChunkSource::Patches {
            footprint: vec![Floor {
                x1: 0.0,
                x2: 10.0,
                z1: 0.0,
                z2: 10.0,
                y: 0.0,
                thickness: 0.2,
                level: 0,
                carrier: CarrierId::WORLD,
            }]
            .into(),
        },
        lod: GrassLod::Near,
        origin: Vec3::ZERO,
        green: Color::srgb(0.1, 0.4, 0.1),
    }
}

#[test]
fn streaming_and_burn_jobs_share_one_inflight_limit() {
    let mut app = fixtures::app();
    app.add_systems(Update, grass_chunk_build_system);
    for _ in 0..GRASS_BUILD_MAX_ACTIVE + 10 {
        app.world_mut().spawn((visual(), GrassChunkBuild::default()));
    }
    for _ in 0..3 {
        app.update();
        let active = app
            .world_mut()
            .query::<&GrassChunkBuild>()
            .iter(app.world())
            .filter(|job| job.task.is_some())
            .count();
        assert_eq!(active, GRASS_BUILD_MAX_ACTIVE);
    }
}

#[test]
fn the_grass_nearest_the_viewer_builds_first_and_near_chunks_before_mid() {
    let mut app = fixtures::app();
    app.add_systems(Update, grass_chunk_build_system);
    app.world_mut().spawn((
        MainCameraMarker,
        GlobalTransform::from_translation(Vec3::new(500.0, 2.0, 0.0)),
    ));
    let chunk = |x: f32, lod| GrassChunkVisual {
        origin: Vec3::new(x, 0.0, 0.0),
        lod,
        ..visual()
    };
    for index in 0..GRASS_BUILD_MAX_ACTIVE {
        app.world_mut()
            .spawn((chunk(index as f32 * 10.0, GrassLod::Near), GrassChunkBuild::default()));
    }
    let underfoot = app
        .world_mut()
        .spawn((chunk(500.0, GrassLod::Near), GrassChunkBuild::default()))
        .id();
    let mid_underfoot = app
        .world_mut()
        .spawn((chunk(500.0, GrassLod::Mid), GrassChunkBuild::default()))
        .id();
    app.update();
    let started = |app: &App, entity| app.world().get::<GrassChunkBuild>(entity).expect("job").task.is_some();
    assert!(started(&app, underfoot));
    assert!(!started(&app, mid_underfoot), "a mid chunk jumped the near queue");
}

#[test]
fn completion_budget_rejects_stale_results_and_keeps_latest_input_queued() {
    let mut app = fixtures::app();
    app.insert_resource(Assets::<Mesh>::default())
        .add_systems(Update, grass_chunk_finish_system);
    let mut entities = Vec::new();
    for _ in 0..GRASS_BUILD_INSTALLS_PER_FRAME + 3 {
        let visual = visual();
        let ready = grass_chunk_mesh(&visual, &[]);
        entities.push(
            app.world_mut()
                .spawn((
                    visual,
                    GrassChunkBuild {
                        ready: Some(ready),
                        ..default()
                    },
                ))
                .id(),
        );
    }
    let stale = entities[0];
    app.world_mut()
        .get_mut::<GrassChunkVisual>(stale)
        .expect("visual")
        .revision = 2;
    app.update();
    assert_eq!(
        app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
        GRASS_BUILD_INSTALLS_PER_FRAME
    );
    assert!(app.world().get::<Mesh3d>(stale).is_none());
    let job = app
        .world()
        .get::<GrassChunkBuild>(stale)
        .expect("latest revision should remain queued");
    assert!(job.ready.is_none() && job.task.is_none());
    app.update();
    assert_eq!(
        app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
        GRASS_BUILD_INSTALLS_PER_FRAME + 2
    );
}

#[test]
fn explosion_burst_coalesces_fades_before_background_dispatch() {
    use crate::{
        map::grass::burn::{GrassBurn, grass_burn_system},
        vfx::ClipRegion,
    };
    use std::time::Instant;
    let mut app = fixtures::app();
    app.add_systems(Update, grass_burn_system);
    let chunks: Vec<_> = (0..32).map(|_| app.world_mut().spawn(visual()).id()).collect();
    let burn = app
        .world_mut()
        .spawn(GrassBurn::new(
            CarrierId::WORLD,
            Vec3::ONE.with_y(0.0),
            10.0,
            0.0,
            0,
            ClipRegion::default(),
        ))
        .id();
    app.update();
    let start = Instant::now();
    for intensity in [0.99, 0.98, 0.5, 0.49] {
        app.world_mut()
            .get_mut::<GrassBurn>(burn)
            .expect("burn")
            .set_intensity(intensity);
        app.update();
    }
    let queued_time = start.elapsed();
    let start = Instant::now();
    for chunk in &chunks {
        let visual = app.world().get::<GrassChunkVisual>(*chunk).expect("visual");
        assert!(grass_chunk_mesh(visual, &visual.burns).is_some());
    }
    eprintln!(
        "32 full grass chunks: synchronous mesh work {:?}; four burn updates queued in {queued_time:?}",
        start.elapsed()
    );
    for chunk in chunks {
        let visual = app.world().get::<GrassChunkVisual>(chunk).expect("visual");
        assert_eq!(visual.revision, 2);
        assert_eq!(visual.burns.len(), 1);
        let job = app.world().get::<GrassChunkBuild>(chunk).expect("one queued build");
        assert!(job.task.is_none() && job.ready.is_none());
    }
}

#[test]
fn terrain_and_exterior_meshes_apply_the_same_physical_clearance() {
    use crate::map::grass::clearance::GrassClearance;
    use bevy::mesh::VertexAttributeValues;
    use common::{
        map::{Grounds, GroundsSettings},
        protocol::{MapLayout, Ramp, RampDirection, RampShape},
    };
    use std::sync::Arc;
    let ramp = Ramp {
        x1: 0.0,
        x2: 10.0,
        z1: 0.0,
        z2: 10.0,
        y: 0.0,
        height: 4.0,
        thickness: 0.2,
        direction: RampDirection::East,
        shape: RampShape::Plank,
        level: 0,
        levels: 1,
        carrier: CarrierId::WORLD,
    };
    let clearance = Arc::new(GrassClearance::new(
        &MapLayout {
            ramps: vec![ramp],
            ..default()
        },
        CarrierId::WORLD,
    ));
    let mut visual = visual();
    visual.patches[0].x2 = 10.0;
    visual.patches[0].z2 = 10.0;
    visual.lod = GrassLod::Near;
    let sources = [
        GrassChunkSource::Patches {
            footprint: vec![Floor {
                x1: 0.0,
                x2: 10.0,
                z1: 0.0,
                z2: 10.0,
                y: 0.0,
                thickness: 0.2,
                level: 0,
                carrier: CarrierId::WORLD,
            }]
            .into(),
        },
        GrassChunkSource::Grounds {
            grounds: Grounds::new([(-10.0, 0.0, 0.0, 10.0)], 0.0, GroundsSettings { level: 0 }),
            cell: IVec2::ZERO,
        },
    ];
    for source in sources {
        visual.source = source;
        visual.clearance = default();
        let baseline = grass_chunk_mesh(&visual, &[]).expect("healthy meadow");
        visual.clearance = clearance.clone();
        let mesh = grass_chunk_mesh(&visual, &[]).expect("grass under high end");
        assert!(mesh.count_vertices() < baseline.count_vertices());
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        for blade in positions.chunks_exact(5) {
            assert!(clearance.allows((Vec3::from(blade[0]) + Vec3::from(blade[1])) * 0.5));
        }
    }
}
