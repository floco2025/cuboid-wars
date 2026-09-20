use std::{f32::consts::TAU, sync::Arc};

use bevy::{mesh::VertexAttributeValues, prelude::*};
use common::protocol::{CarrierId, Floor};

use super::{
    burn::{BURN_VERTICAL_TOLERANCE, GrassBurn, grass_burn_system},
    fixtures, jobs,
    mesh::{BLADE_HEIGHT_MAX, GrassLod, MID_SWAY_WEIGHT, VERTICES_PER_BLADE, WIND_SWAY_FACTOR, grass_patch_mesh},
    patch::GrassPatch,
    sources::GrassChunkSource,
    streaming::{GrassChunkVisual, grass_chunk_mesh, padded_grass_bounds},
};
use crate::{
    constants::{
        EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR, GRASS_WIND_STRENGTH,
    },
    map::terrain::TerrainCover,
    test_fixtures::CELL,
    vfx::ClipRegion,
};

fn patch_contains_base(patch: GrassPatch, x: f32, z: f32) -> bool {
    const EPSILON: f32 = 0.0001;
    x >= patch.x1 - EPSILON && x <= patch.x2 + EPSILON && z >= patch.z1 - EPSILON && z <= patch.z2 + EPSILON
}

fn test_patch() -> GrassPatch {
    GrassPatch {
        x1: CELL * 2.0,
        x2: CELL * 3.0,
        y: 0.0,
        z1: -CELL * 2.0,
        z2: -CELL,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn patch_floor(patch: GrassPatch) -> Floor {
    Floor {
        x1: patch.x1,
        x2: patch.x2,
        z1: patch.z1,
        z2: patch.z2,
        y: patch.y,
        thickness: 0.2,
        level: patch.level,
        carrier: patch.carrier,
    }
}

fn patch_mesh(patch: GrassPatch, lod: GrassLod, burns: &[GrassBurn]) -> Mesh {
    grass_patch_mesh(patch, &[patch_floor(patch)], lod, test_green(), burns, &default())
}

fn test_green() -> Color {
    Color::srgb_u8(0x27, 0x73, 0x31)
}

fn positions(mesh: &Mesh) -> &[[f32; 3]] {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(values)) => values,
        _ => panic!("grass mesh positions missing or not Float32x3"),
    }
}

fn uvs(mesh: &Mesh) -> &[[f32; 2]] {
    match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(values)) => values,
        _ => panic!("grass mesh uvs missing or not Float32x2"),
    }
}

fn colors(mesh: &Mesh) -> &[[f32; 4]] {
    match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(values)) => values,
        _ => panic!("grass mesh colors missing or not Float32x4"),
    }
}

fn average_rgb(values: &[[f32; 4]]) -> f32 {
    values.iter().map(|color| color[0] + color[1] + color[2]).sum::<f32>() / values.len() as f32
}

fn max_y(values: &[[f32; 3]]) -> f32 {
    values
        .iter()
        .map(|position| position[1])
        .fold(f32::NEG_INFINITY, f32::max)
}

#[test]
fn same_patch_and_lod_produce_identical_mesh() {
    let patch = test_patch();
    let first = patch_mesh(patch, GrassLod::Near, &[]);
    let second = patch_mesh(patch, GrassLod::Near, &[]);
    assert_eq!(positions(&first), positions(&second));
    assert_eq!(uvs(&first), uvs(&second));
    assert_eq!(colors(&first), colors(&second));
}

#[test]
fn configured_green_changes_the_blade_colors() {
    let patch = test_patch();
    let green = grass_patch_mesh(
        patch,
        &[patch_floor(patch)],
        GrassLod::Near,
        Color::srgb_u8(0x10, 0x80, 0x20),
        &[],
        &default(),
    );
    let blue = grass_patch_mesh(
        patch,
        &[patch_floor(patch)],
        GrassLod::Near,
        Color::srgb_u8(0x10, 0x20, 0x80),
        &[],
        &default(),
    );
    assert_eq!(positions(&green), positions(&blue));
    assert_ne!(colors(&green), colors(&blue));
}

#[test]
fn near_lod_is_denser_than_mid_lod() {
    let patch = test_patch();
    let near = patch_mesh(patch, GrassLod::Near, &[]);
    let mid = patch_mesh(patch, GrassLod::Mid, &[]);
    assert!(positions(&near).len() > positions(&mid).len() * 3);
    assert!(GrassLod::Near.tuft_count(patch.area()) > GrassLod::Mid.tuft_count(patch.area()));
}

#[test]
fn generated_blades_never_root_in_brown_soil() {
    let mesh = patch_mesh(test_patch(), GrassLod::Near, &[]);
    for blade in positions(&mesh).chunks_exact(VERTICES_PER_BLADE) {
        let root = Vec2::new((blade[0][0] + blade[1][0]) * 0.5, (blade[0][2] + blade[1][2]) * 0.5);
        assert!(TerrainCover::at(root).soil < 0.28);
    }
}

#[test]
fn burned_grass_remains_visible_short_dark_and_still() {
    let patch = test_patch();
    let normal = patch_mesh(patch, GrassLod::Near, &[]);
    let burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(
            f32::midpoint(patch.x1, patch.x2),
            patch.y,
            f32::midpoint(patch.z1, patch.z2),
        ),
        CELL * 4.0,
        0.7,
        3,
        ClipRegion::default(),
    );
    let burned = patch_mesh(patch, GrassLod::Near, &[burn]);

    assert!(!positions(&burned).is_empty());
    assert_eq!(positions(&burned).len(), positions(&normal).len());
    let max_height = positions(&burned)
        .iter()
        .map(|position| position[1] - patch.y)
        .fold(0.0_f32, f32::max);
    assert!(max_height <= BLADE_HEIGHT_MAX * EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR + 0.001);
    let max_sway = uvs(&burned).iter().map(|uv| uv[0]).fold(0.0_f32, f32::max);
    assert!(max_sway <= EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR + f32::EPSILON);
    assert!(average_rgb(colors(&burned)) < average_rgb(colors(&normal)) * 0.35);
}

#[test]
fn recovering_grass_interpolates_between_burned_and_healthy() {
    let patch = test_patch();
    let normal = patch_mesh(patch, GrassLod::Near, &[]);
    let mut burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(
            f32::midpoint(patch.x1, patch.x2),
            patch.y,
            f32::midpoint(patch.z1, patch.z2),
        ),
        CELL * 4.0,
        0.7,
        3,
        ClipRegion::default(),
    );
    let burned = patch_mesh(patch, GrassLod::Near, std::slice::from_ref(&burn));
    burn.set_intensity(0.5);
    let recovering = patch_mesh(patch, GrassLod::Near, &[burn]);
    assert!(max_y(positions(&burned)) < max_y(positions(&recovering)));
    assert!(max_y(positions(&recovering)) < max_y(positions(&normal)));
    assert!(average_rgb(colors(&burned)) < average_rgb(colors(&recovering)));
    assert!(average_rgb(colors(&recovering)) < average_rgb(colors(&normal)));
}

#[test]
fn different_scorch_variants_produce_different_burn_outlines() {
    let center = Vec3::ZERO;
    let first = GrassBurn::new(CarrierId::WORLD, center, 10.0, 0.4, 0, ClipRegion::default());
    let second = GrassBurn::new(CarrierId::WORLD, center, 10.0, 0.4, 1, ClipRegion::default());
    let samples = |burn: &GrassBurn| {
        (0..32)
            .map(|index| {
                let angle = index as f32 / 32.0 * TAU;
                burn.strength_at(Vec3::new(angle.cos() * 8.0, 0.0, angle.sin() * 8.0))
            })
            .collect::<Vec<_>>()
    };
    assert_ne!(samples(&first), samples(&second));
}

#[test]
fn burn_on_another_level_does_not_change_grass() {
    let patch = test_patch();
    let normal = patch_mesh(patch, GrassLod::Near, &[]);
    let burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(
            f32::midpoint(patch.x1, patch.x2),
            patch.y + BURN_VERTICAL_TOLERANCE * 2.0,
            f32::midpoint(patch.z1, patch.z2),
        ),
        CELL * 4.0,
        0.0,
        0,
        ClipRegion::default(),
    );
    let other_level = patch_mesh(patch, GrassLod::Near, &[burn]);
    assert_eq!(positions(&normal), positions(&other_level));
    assert_eq!(colors(&normal), colors(&other_level));
}

#[test]
fn removing_burn_restores_original_chunk_mesh() {
    let patch = test_patch();
    let origin = Vec3::new(
        f32::midpoint(patch.x1, patch.x2),
        patch.y,
        f32::midpoint(patch.z1, patch.z2),
    );
    let footprint: Arc<[Floor]> = vec![patch_floor(patch)].into();
    let visual = GrassChunkVisual {
        revision: 0,
        burns: Vec::new(),
        clearance: default(),
        patches: vec![patch],
        source: GrassChunkSource::Patches { footprint },
        lod: GrassLod::Near,
        origin,
        green: test_green(),
    };
    let baseline = grass_chunk_mesh(&visual, &[]).expect("grass expected");
    let expected_positions = positions(&baseline).to_vec();

    let mut app = fixtures::app();
    app.insert_resource(Assets::<Mesh>::default()).add_systems(
        Update,
        (
            grass_burn_system,
            jobs::grass_chunk_finish_system,
            jobs::grass_chunk_build_system,
        )
            .chain(),
    );
    let chunk = app.world_mut().spawn((visual, jobs::GrassChunkBuild::default())).id();
    let burn_entity = app
        .world_mut()
        .spawn(GrassBurn::new(
            CarrierId::WORLD,
            Vec3::new(
                f32::midpoint(patch.x1, patch.x2),
                patch.y,
                f32::midpoint(patch.z1, patch.z2),
            ),
            CELL * 4.0,
            0.0,
            0,
            ClipRegion::default(),
        ))
        .id();
    settle(&mut app);
    let mesh_handle = app.world().get::<Mesh3d>(chunk).expect("burned initial mesh").0.clone();
    assert_ne!(
        positions(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .expect("burned grass mesh missing"),
        ),
        expected_positions
    );
    app.world_mut().entity_mut(burn_entity).despawn();
    settle(&mut app);
    assert_eq!(
        positions(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .expect("recovered grass mesh missing"),
        ),
        expected_positions
    );
}

#[test]
fn root_vertices_have_zero_sway_and_stay_on_the_terrain_footprint() {
    let patch = test_patch();
    let mesh = patch_mesh(patch, GrassLod::Near, &[]);
    for (position, uv) in positions(&mesh).iter().zip(uvs(&mesh)) {
        match uv[0] {
            0.0 => {
                assert!((position[1] - patch.y).abs() < f32::EPSILON);
                assert!(patch_contains_base(patch, position[0], position[2]));
            }
            MID_SWAY_WEIGHT | 1.0 => assert!(position[1] > patch.y),
            weight => panic!("unexpected sway weight {weight}"),
        }
    }
}

#[test]
fn narrow_trim_patch_still_receives_grass() {
    let patch = GrassPatch {
        x1: 0.0,
        x2: 0.15,
        z1: -5.0,
        z2: 5.0,
        y: 0.0,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let mesh = patch_mesh(patch, GrassLod::Near, &[]);
    assert!(!positions(&mesh).is_empty());
    for blade in positions(&mesh).chunks_exact(VERTICES_PER_BLADE) {
        assert!(patch_contains_base(patch, blade[0][0], blade[0][2]));
        assert!(patch_contains_base(patch, blade[1][0], blade[1][2]));
    }
}

#[test]
fn padded_chunk_bounds_contain_full_sway() {
    let patch = test_patch();
    let mesh = patch_mesh(patch, GrassLod::Near, &[]);
    let aabb = padded_grass_bounds(&mesh);
    let max_sway = GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR;
    for position in positions(&mesh) {
        let swayed_min = Vec3::from_array(*position) - Vec3::new(max_sway, 0.0, max_sway);
        let swayed_max = Vec3::from_array(*position) + Vec3::new(max_sway, 0.0, max_sway);
        assert!(swayed_min.cmpge(aabb.min().into()).all());
        assert!(swayed_max.cmple(aabb.max().into()).all());
    }
}

fn settle(app: &mut App) {
    for _ in 0..1000 {
        app.update();
        if app
            .world_mut()
            .query::<&jobs::GrassChunkBuild>()
            .iter(app.world())
            .next()
            .is_none()
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("grass builds did not settle");
}
