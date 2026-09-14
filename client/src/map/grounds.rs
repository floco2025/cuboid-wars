use std::collections::BTreeMap;

use crate::{
    config::{AssetSet, ClientSettings},
    constants::{DECORATION_FAR_CHUNK_SIZE, DECORATION_FAR_FADE, DECORATION_NEAR_BAND},
    map::grass::GrassMaterials,
    materials::TreeMaterial,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::{
    map::{DecorationKind, GroundDecoration, RockClass},
    protocol::MapLayout,
};

use super::{rocks::RockAssets, trees::TreeAssets};

#[derive(Component)]
pub(super) struct GroundsVisual;

pub(super) fn grounds_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tree_materials: ResMut<Assets<TreeMaterial>>,
    grass_materials: Res<GrassMaterials>,
    existing: Query<Entity, With<GroundsVisual>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let Some(grounds) = &layout.grounds else { return };
    let material = grass_materials.terrain.clone();
    let terrain = grounds.mesh();
    let positions: Vec<[f32; 3]> = terrain.vertices.iter().map(|v| v.to_array()).collect();
    let uvs: Vec<[f32; 2]> = terrain.vertices.iter().map(|v| [v.x, v.z]).collect();
    let normals: Vec<[f32; 3]> = terrain
        .vertices
        .iter()
        .map(|v| grounds.normal(v.x, v.z).to_array())
        .collect();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(terrain.triangles.into_iter().flatten().collect()));
    commands.spawn((
        GroundsVisual,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));

    let trees = TreeAssets::new(&server, &mut meshes, &mut materials, &mut tree_materials);
    let rocks = RockAssets::new(&server, &asset_set, &settings, &mut meshes, &mut materials);
    let mut far: BTreeMap<(i32, i32), Vec<GroundDecoration>> = BTreeMap::new();
    for decoration in grounds.decorations() {
        let outside = grounds.distance_outside_map(decoration.position.x, decoration.position.z);
        let merges_far = decoration.kind != DecorationKind::Rock(RockClass::Pebble);
        if merges_far && outside > DECORATION_NEAR_BAND {
            let key = (
                (decoration.position.x / DECORATION_FAR_CHUNK_SIZE).floor() as i32,
                (decoration.position.z / DECORATION_FAR_CHUNK_SIZE).floor() as i32,
            );
            far.entry(key).or_default().push(decoration);
            continue;
        }
        let root = commands
            .spawn((
                GroundsVisual,
                Transform::from_translation(decoration.position)
                    .with_scale(decoration.scale)
                    .with_rotation(decoration.rotation),
                Visibility::Visible,
            ))
            .id();
        match decoration.kind {
            DecorationKind::Tree => trees.spawn(&mut commands, root, &decoration),
            DecorationKind::Rock(class) => rocks.spawn(&mut commands, root, &decoration, class),
        }
    }
    let (far_bark, far_foliage) = trees.far_materials();
    let far_stone = rocks.material();
    for ((chunk_x, chunk_z), group) in far {
        let x = (chunk_x as f32 + 0.5) * DECORATION_FAR_CHUNK_SIZE;
        let z = (chunk_z as f32 + 0.5) * DECORATION_FAR_CHUNK_SIZE;
        let origin = Vec3::new(x, grounds.height(x, z), z);
        let mut chunk_meshes = Vec::new();
        if let Some((wood, leaves)) = trees.far_chunk(&group, origin) {
            chunk_meshes.push((wood, far_bark.clone()));
            chunk_meshes.push((leaves, far_foliage.clone()));
        }
        if let Some(stone) = rocks.far_chunk(&group, origin) {
            chunk_meshes.push((stone, far_stone.clone()));
        }
        for (mesh, material) in chunk_meshes {
            commands.spawn((
                GroundsVisual,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::from_translation(origin),
                NotShadowCaster,
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: DECORATION_FAR_FADE[0]..DECORATION_FAR_FADE[1],
                    use_aabb: true,
                },
            ));
        }
    }
}
