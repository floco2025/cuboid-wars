use std::f32::consts::TAU;

use bevy::{mesh::VertexAttributeValues, prelude::*};
use common::protocol::{CarrierId, TerrainCell};

use super::{
    burn::{BURN_VERTICAL_TOLERANCE, GrassBurn, grass_burn_system},
    mesh::{
        BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, GrassLod, MID_SWAY_WEIGHT, VERTICES_PER_BLADE, WIND_SWAY_FACTOR,
        cell_tuft_count, grass_cell_mesh,
    },
    spawn::{GrassChunkVisual, OpenEdges, grass_chunk_mesh, terrain_cell_aabb},
};
use crate::{
    constants::{EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR},
    map::terrain_surface::TerrainCover,
    test_fixtures::{CELL, map_settings},
    vfx::ClipRegion,
};

fn test_cell() -> TerrainCell {
    TerrainCell {
        x: CELL * 2.5,
        y: 0.0,
        z: -CELL * 1.5,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

const ALL_OPEN: OpenEdges = OpenEdges {
    pos_x: true,
    neg_x: true,
    pos_z: true,
    neg_z: true,
};
const ALL_CLOSED: OpenEdges = OpenEdges {
    pos_x: false,
    neg_x: false,
    pos_z: false,
    neg_z: false,
};

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
fn same_cell_and_lod_produce_identical_mesh() {
    let first = grass_cell_mesh(test_cell(), CELL, GrassLod::Near, ALL_OPEN, &[]);
    let second = grass_cell_mesh(test_cell(), CELL, GrassLod::Near, ALL_OPEN, &[]);
    assert_eq!(positions(&first), positions(&second));
    assert_eq!(uvs(&first), uvs(&second));
    assert_eq!(colors(&first), colors(&second));
}

#[test]
fn near_lod_is_denser_than_mid_lod() {
    let near = grass_cell_mesh(test_cell(), CELL, GrassLod::Near, ALL_OPEN, &[]);
    let mid = grass_cell_mesh(test_cell(), CELL, GrassLod::Mid, ALL_OPEN, &[]);
    assert!(positions(&near).len() > positions(&mid).len() * 3);
    assert!(cell_tuft_count(GrassLod::Near, CELL) > cell_tuft_count(GrassLod::Mid, CELL));
}

#[test]
fn generated_blades_never_root_in_brown_soil() {
    let mesh = grass_cell_mesh(test_cell(), CELL, GrassLod::Near, ALL_OPEN, &[]);
    for blade in positions(&mesh).chunks_exact(VERTICES_PER_BLADE) {
        let root = Vec2::new((blade[0][0] + blade[1][0]) * 0.5, (blade[0][2] + blade[1][2]) * 0.5);
        assert!(TerrainCover::at(root).soil < 0.28);
    }
}

#[test]
fn burned_grass_remains_visible_short_dark_and_still() {
    let cell = test_cell();
    let normal = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[]);
    let burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(cell.x, cell.y, cell.z),
        CELL * 4.0,
        0.7,
        3,
        ClipRegion::default(),
    );
    let burned = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[burn]);

    assert!(!positions(&burned).is_empty());
    assert_eq!(positions(&burned).len(), positions(&normal).len());
    let max_height = positions(&burned)
        .iter()
        .map(|position| position[1] - cell.y)
        .fold(0.0_f32, f32::max);
    assert!(max_height <= BLADE_HEIGHT_MAX * EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR + 0.001);
    let max_sway = uvs(&burned).iter().map(|uv| uv[0]).fold(0.0_f32, f32::max);
    assert!(max_sway <= EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR + f32::EPSILON);
    assert!(average_rgb(colors(&burned)) < average_rgb(colors(&normal)) * 0.35);
}

#[test]
fn recovering_grass_interpolates_between_burned_and_healthy() {
    let cell = test_cell();
    let normal = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[]);
    let mut burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(cell.x, cell.y, cell.z),
        CELL * 4.0,
        0.7,
        3,
        ClipRegion::default(),
    );
    let burned = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, std::slice::from_ref(&burn));
    burn.set_intensity(0.5);
    let recovering = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[burn]);
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
    let cell = test_cell();
    let normal = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[]);
    let burn = GrassBurn::new(
        CarrierId::WORLD,
        Vec3::new(cell.x, cell.y + BURN_VERTICAL_TOLERANCE * 2.0, cell.z),
        CELL * 4.0,
        0.0,
        0,
        ClipRegion::default(),
    );
    let other_level = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[burn]);
    assert_eq!(positions(&normal), positions(&other_level));
    assert_eq!(colors(&normal), colors(&other_level));
}

#[test]
fn removing_burn_restores_original_chunk_mesh() {
    let cell = test_cell();
    let origin = Vec3::new(cell.x, cell.y, cell.z);
    let cells = vec![(cell, ALL_OPEN)];
    let baseline = grass_chunk_mesh(&cells, CELL, GrassLod::Near, origin, &[]).expect("grass expected");
    let expected_positions = positions(&baseline).to_vec();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(map_settings())
        .insert_resource(Assets::<Mesh>::default())
        .add_systems(Update, grass_burn_system);
    let mesh_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(baseline);
    app.world_mut().spawn((
        GrassChunkVisual {
            cells,
            lod: GrassLod::Near,
            origin,
        },
        Mesh3d(mesh_handle.clone()),
    ));
    let burn_entity = app
        .world_mut()
        .spawn(GrassBurn::new(
            CarrierId::WORLD,
            Vec3::new(cell.x, cell.y, cell.z),
            CELL * 4.0,
            0.0,
            0,
            ClipRegion::default(),
        ))
        .id();
    app.update();
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
    app.update();
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
fn root_vertices_have_zero_sway_and_closed_edges_clip_overhang() {
    let cell = test_cell();
    let mesh = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_CLOSED, &[]);
    let bound = CELL / 2.0;
    for (position, uv) in positions(&mesh).iter().zip(uvs(&mesh)) {
        assert!((position[0] - cell.x).abs() <= bound);
        assert!((position[2] - cell.z).abs() <= bound);
        match uv[0] {
            0.0 => assert!((position[1] - cell.y).abs() < f32::EPSILON),
            MID_SWAY_WEIGHT | 1.0 => assert!(position[1] > cell.y),
            weight => panic!("unexpected sway weight {weight}"),
        }
    }
}

#[test]
fn padded_cell_bounds_contain_full_sway() {
    let cell = test_cell();
    let mesh = grass_cell_mesh(cell, CELL, GrassLod::Near, ALL_OPEN, &[]);
    let aabb = terrain_cell_aabb(cell, CELL);
    let bound = CELL / 2.0 + BLADE_MAX_OVERHANG;
    let max_sway = crate::constants::GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR;
    for position in positions(&mesh) {
        assert!((position[0] - cell.x).abs() <= bound);
        assert!((position[2] - cell.z).abs() <= bound);
        let swayed_min = Vec3::from_array(*position) - Vec3::new(max_sway, 0.0, max_sway);
        let swayed_max = Vec3::from_array(*position) + Vec3::new(max_sway, 0.0, max_sway);
        assert!(swayed_min.cmpge(aabb.min().into()).all());
        assert!(swayed_max.cmple(aabb.max().into()).all());
    }
}
