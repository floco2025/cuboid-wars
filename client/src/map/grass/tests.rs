use std::f32::consts::TAU;

use bevy::{mesh::VertexAttributeValues, prelude::*};

use super::burn::{GrassBurn, grass_burn_system};
use super::mesh::{
    BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, BLADES_PER_TUFT, INDICES_PER_BLADE, MID_SWAY_WEIGHT, VERTICES_PER_BLADE,
    WIND_SWAY_FACTOR, burn_strength_at, cell_tuft_count, grass_cell_mesh,
};
use super::spawn::{GrassCellVisual, OpenEdges, grass_cell_aabb};
use crate::config::{ClientSettings, GrassConfig};
use crate::constants::{
    EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR,
    EXPLOSION_GRASS_BURN_VERTICAL_TOLERANCE,
};
use common::{constants::GRID_CELL_SIZE, protocol::GrassCell};

fn test_cell() -> GrassCell {
    GrassCell {
        x: GRID_CELL_SIZE * 2.5,
        y: 0.0,
        z: -GRID_CELL_SIZE * 1.5,
        level: 0,
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
fn same_cell_produces_identical_mesh() {
    let config = GrassConfig::default();
    let first = grass_cell_mesh(test_cell(), &config, ALL_OPEN, &[]);
    let second = grass_cell_mesh(test_cell(), &config, ALL_OPEN, &[]);
    assert_eq!(positions(&first), positions(&second));
    assert_eq!(uvs(&first), uvs(&second));
    assert_eq!(colors(&first), colors(&second));
}

#[test]
fn burned_grass_remains_visible_short_dark_and_still() {
    let cell = test_cell();
    let config = GrassConfig::default();
    let normal = grass_cell_mesh(cell, &config, ALL_OPEN, &[]);
    let burn = GrassBurn::new(Vec3::new(cell.x, cell.y, cell.z), GRID_CELL_SIZE * 4.0, 0.7, 3);
    let burned = grass_cell_mesh(cell, &config, ALL_OPEN, &[burn]);

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
    for blade in colors(&burned).chunks_exact(VERTICES_PER_BLADE) {
        assert!(average_rgb(&blade[0..1]) < average_rgb(&blade[2..3]));
        assert!(average_rgb(&blade[2..3]) < average_rgb(&blade[4..5]));
    }
}

#[test]
fn recovering_grass_interpolates_between_burned_and_healthy() {
    let cell = test_cell();
    let config = GrassConfig::default();
    let normal = grass_cell_mesh(cell, &config, ALL_OPEN, &[]);
    let mut burn = GrassBurn::new(Vec3::new(cell.x, cell.y, cell.z), GRID_CELL_SIZE * 4.0, 0.7, 3);
    let burned = grass_cell_mesh(cell, &config, ALL_OPEN, &[burn]);
    burn.set_intensity(0.5);
    let recovering = grass_cell_mesh(cell, &config, ALL_OPEN, &[burn]);

    assert_eq!(positions(&burned).len(), positions(&recovering).len());
    assert_eq!(positions(&recovering).len(), positions(&normal).len());
    assert!(max_y(positions(&burned)) < max_y(positions(&recovering)));
    assert!(max_y(positions(&recovering)) < max_y(positions(&normal)));
    assert!(average_rgb(colors(&burned)) < average_rgb(colors(&recovering)));
    assert!(average_rgb(colors(&recovering)) < average_rgb(colors(&normal)));
    let max_burned_sway = uvs(&burned).iter().map(|uv| uv[0]).fold(0.0_f32, f32::max);
    let max_recovering_sway = uvs(&recovering).iter().map(|uv| uv[0]).fold(0.0_f32, f32::max);
    assert!(max_burned_sway < max_recovering_sway);
    assert!(max_recovering_sway < 1.0);
}

#[test]
fn different_scorch_variants_produce_different_burn_outlines() {
    let center = Vec3::ZERO;
    let first = GrassBurn::new(center, 10.0, 0.4, 0);
    let second = GrassBurn::new(center, 10.0, 0.4, 1);
    let first_samples: Vec<f32> = (0..32)
        .map(|index| {
            let angle = index as f32 / 32.0 * TAU;
            burn_strength_at(Vec3::new(angle.cos() * 8.0, 0.0, angle.sin() * 8.0), first)
        })
        .collect();
    let second_samples: Vec<f32> = (0..32)
        .map(|index| {
            let angle = index as f32 / 32.0 * TAU;
            burn_strength_at(Vec3::new(angle.cos() * 8.0, 0.0, angle.sin() * 8.0), second)
        })
        .collect();

    assert_ne!(first_samples, second_samples);
    assert!(first_samples.windows(2).any(|pair| (pair[0] - pair[1]).abs() > 0.05));
}

#[test]
fn burn_on_another_level_does_not_change_grass() {
    let cell = test_cell();
    let config = GrassConfig::default();
    let normal = grass_cell_mesh(cell, &config, ALL_OPEN, &[]);
    let burn = GrassBurn::new(
        Vec3::new(cell.x, cell.y + EXPLOSION_GRASS_BURN_VERTICAL_TOLERANCE * 2.0, cell.z),
        GRID_CELL_SIZE * 4.0,
        0.0,
        0,
    );
    let other_level = grass_cell_mesh(cell, &config, ALL_OPEN, &[burn]);

    assert_eq!(positions(&normal), positions(&other_level));
    assert_eq!(uvs(&normal), uvs(&other_level));
    assert_eq!(colors(&normal), colors(&other_level));
}

#[test]
fn weaker_overlapping_burn_does_not_override_stronger_burn() {
    let cell = test_cell();
    let config = GrassConfig::default();
    let center = Vec3::new(cell.x, cell.y, cell.z);
    let strong = GrassBurn::new(center, GRID_CELL_SIZE * 4.0, 0.0, 0);
    let weak = GrassBurn::new(center, GRID_CELL_SIZE, 1.0, 1);
    let strong_only = grass_cell_mesh(cell, &config, ALL_OPEN, &[strong]);
    let overlapping = grass_cell_mesh(cell, &config, ALL_OPEN, &[weak, strong]);

    assert_eq!(positions(&strong_only), positions(&overlapping));
    assert_eq!(uvs(&strong_only), uvs(&overlapping));
    assert_eq!(colors(&strong_only), colors(&overlapping));
}

#[test]
fn removing_burn_restores_original_grass_mesh() {
    let settings = ClientSettings::load_default().expect("default client config should load");
    let cell = test_cell();
    let baseline = grass_cell_mesh(cell, &settings.grass, ALL_OPEN, &[]);
    let expected_positions = positions(&baseline).to_vec();
    let expected_uvs = uvs(&baseline).to_vec();
    let expected_colors = colors(&baseline).to_vec();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(Assets::<Mesh>::default())
        .add_systems(Update, grass_burn_system);
    let mesh_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(baseline);
    app.world_mut()
        .spawn((GrassCellVisual { cell, open: ALL_OPEN }, Mesh3d(mesh_handle.clone())));
    let burn_entity = app
        .world_mut()
        .spawn(GrassBurn::new(
            Vec3::new(cell.x, cell.y, cell.z),
            GRID_CELL_SIZE * 4.0,
            0.0,
            0,
        ))
        .id();

    app.update();
    let burned_max_y;
    {
        let meshes = app.world().resource::<Assets<Mesh>>();
        let burned = meshes.get(&mesh_handle).expect("grass mesh should still exist");
        assert_eq!(positions(burned).len(), expected_positions.len());
        assert_ne!(positions(burned), expected_positions);
        burned_max_y = max_y(positions(burned));
    }

    app.world_mut()
        .get_mut::<GrassBurn>(burn_entity)
        .expect("burn footprint should still exist")
        .set_intensity(0.5);
    app.update();
    {
        let meshes = app.world().resource::<Assets<Mesh>>();
        let recovering = meshes.get(&mesh_handle).expect("grass mesh should still exist");
        assert_eq!(positions(recovering).len(), expected_positions.len());
        assert!(max_y(positions(recovering)) > burned_max_y);
        assert!(max_y(positions(recovering)) < max_y(&expected_positions));
    }

    app.world_mut().entity_mut(burn_entity).despawn();
    app.update();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let restored = meshes.get(&mesh_handle).expect("grass mesh should still exist");
    assert_eq!(positions(restored), expected_positions);
    assert_eq!(uvs(restored), expected_uvs);
    assert_eq!(colors(restored), expected_colors);
}

#[test]
fn blade_count_scales_with_density() {
    let sparse = GrassConfig {
        tufts_per_m2: 2.0,
        ..GrassConfig::default()
    };
    let dense = GrassConfig {
        tufts_per_m2: 4.0,
        ..GrassConfig::default()
    };
    let sparse_mesh = grass_cell_mesh(test_cell(), &sparse, ALL_OPEN, &[]);
    let dense_mesh = grass_cell_mesh(test_cell(), &dense, ALL_OPEN, &[]);
    assert_eq!(
        positions(&sparse_mesh).len(),
        cell_tuft_count(&sparse) * BLADES_PER_TUFT * VERTICES_PER_BLADE
    );
    assert_eq!(
        positions(&dense_mesh).len(),
        cell_tuft_count(&dense) * BLADES_PER_TUFT * VERTICES_PER_BLADE
    );
    assert_eq!(
        sparse_mesh.indices().map_or(0, bevy::mesh::Indices::len),
        cell_tuft_count(&sparse) * BLADES_PER_TUFT * INDICES_PER_BLADE
    );
    assert!(positions(&dense_mesh).len() > positions(&sparse_mesh).len());
}

#[test]
fn root_vertices_have_zero_sway_weight() {
    let cell = test_cell();
    let mesh = grass_cell_mesh(cell, &GrassConfig::default(), ALL_OPEN, &[]);
    for (position, uv) in positions(&mesh).iter().zip(uvs(&mesh)) {
        match uv[0] {
            0.0 => assert!((position[1] - cell.y).abs() < f32::EPSILON),
            MID_SWAY_WEIGHT | 1.0 => assert!(position[1] > cell.y),
            weight => panic!("grass sway weight {weight} is not a root, mid, or tip ring"),
        }
    }
}

#[test]
fn blades_stay_within_cell_plus_overhang() {
    let cell = test_cell();
    let config = GrassConfig::default();
    let mesh = grass_cell_mesh(cell, &config, ALL_OPEN, &[]);
    let aabb = grass_cell_aabb(cell, &config);
    let bound = GRID_CELL_SIZE / 2.0 + BLADE_MAX_OVERHANG;
    let max_sway = config.wind_strength * WIND_SWAY_FACTOR;
    for position in positions(&mesh) {
        assert!((position[0] - cell.x).abs() <= bound);
        assert!((position[2] - cell.z).abs() <= bound);
        assert!(position[1] >= cell.y && position[1] <= cell.y + BLADE_HEIGHT_MAX);

        // The padded AABB must contain every vertex even at full sway.
        let swayed_min = Vec3::from_array(*position) - Vec3::new(max_sway, 0.0, max_sway);
        let swayed_max = Vec3::from_array(*position) + Vec3::new(max_sway, 0.0, max_sway);
        assert!(swayed_min.cmpge(aabb.min().into()).all());
        assert!(swayed_max.cmple(aabb.max().into()).all());
    }
}

#[test]
fn closed_edges_keep_blades_inside_cell() {
    let cell = test_cell();
    let mesh = grass_cell_mesh(cell, &GrassConfig::default(), ALL_CLOSED, &[]);
    let bound = GRID_CELL_SIZE / 2.0;
    for position in positions(&mesh) {
        assert!((position[0] - cell.x).abs() <= bound);
        assert!((position[2] - cell.z).abs() <= bound);
    }
}
