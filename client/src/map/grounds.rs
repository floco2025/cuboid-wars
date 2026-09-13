use crate::{
    config::{AssetSet, ClientSettings},
    constants::GROUNDS_ROCK_COLOR,
    materials::{GrassMaterial, TerrainMaterial, terrain_material},
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::protocol::MapLayout;

use super::{terrain_grass::spawn_terrain_grass, terrain_surface::TerrainCover, trees::TreeAssets};

#[derive(Component)]
pub(super) struct GroundsVisual;

pub(super) fn grounds_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    assets: Res<AssetSet>,
    settings: Res<ClientSettings>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
    mut images: ResMut<Assets<Image>>,
    existing: Query<Entity, With<GroundsVisual>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let Some(grounds) = &layout.grounds else { return };
    let definition = assets.material_by_id(&grounds.settings.material);
    let material = terrain_materials.add(terrain_material(
        &server,
        &definition.textures.base_color,
        images.add(TerrainCover::image()),
        settings.rendering.texture_anisotropy,
        settings.rendering.mipmaps,
    ));
    let terrain = grounds.mesh(true);
    let positions: Vec<[f32; 3]> = terrain.vertices.iter().map(|v| v.to_array()).collect();
    let uvs: Vec<[f32; 2]> = terrain.vertices.iter().map(|v| [v.x, v.z]).collect();
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in &terrain.triangles {
        let normal = (terrain.vertices[b as usize] - terrain.vertices[a as usize])
            .cross(terrain.vertices[c as usize] - terrain.vertices[a as usize]);
        for i in [a, b, c] {
            normals[i as usize] += normal;
        }
    }
    let normals: Vec<[f32; 3]> = normals
        .into_iter()
        .map(|v| v.normalize_or(Vec3::Y).to_array())
        .collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(terrain.triangles.into_iter().flatten().collect()));
    mesh.generate_tangents()
        .expect("grounds mesh has invalid tangent coordinates");
    commands.spawn((
        GroundsVisual,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));

    if settings.grass.enabled {
        spawn_terrain_grass(
            &mut commands,
            grounds,
            &mut meshes,
            &mut grass_materials,
            (settings.grass.tufts_per_m2 / 16.0).min(2.0),
        );
    }

    let trees = TreeAssets::new(&server, &mut meshes, &mut materials);
    let rock = meshes.add(Sphere::new(1.0).mesh().ico(1).expect("rock subdivision invalid"));
    let stone = materials.add(StandardMaterial {
        base_color: GROUNDS_ROCK_COLOR,
        perceptual_roughness: 0.95,
        ..default()
    });
    for (i, decoration) in grounds.decorations().into_iter().enumerate() {
        let root = commands
            .spawn((
                GroundsVisual,
                Transform::from_translation(decoration.position)
                    .with_scale(decoration.scale)
                    .with_rotation(Quat::from_rotation_y(i as f32 * 1.7)),
                Visibility::Visible,
            ))
            .id();
        if decoration.tree {
            trees.spawn(&mut commands, root, i);
        } else {
            commands.spawn((
                ChildOf(root),
                Mesh3d(rock.clone()),
                MeshMaterial3d(stone.clone()),
                Transform::from_xyz(0.0, 0.45, 0.0),
            ));
        }
    }
}
