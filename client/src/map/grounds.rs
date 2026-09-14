use crate::{
    constants::{GROUNDS_ROCK_COLOR, TERRAIN_GRASS_CHUNK_SIZE},
    map::grass::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource, GrassChunks},
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::{
    map::Grounds,
    protocol::{CarrierId, MapLayout},
};

use super::{grass::GrassPatch, trees::TreeAssets};

#[derive(Component)]
pub(super) struct GroundsVisual;

pub(super) fn grounds_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunks: ResMut<GrassChunks>,
    existing: Query<Entity, With<GroundsVisual>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let Some(grounds) = &layout.grounds else { return };
    let material = chunks.terrain_material();
    let terrain = grounds.mesh(true);
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

    register_grounds_grass(&mut chunks, grounds);

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

// One chunk per ten-metre cell outside the map, out to the playable margin
// and a little beyond; the streamer builds them as the camera approaches.
fn register_grounds_grass(chunks: &mut GrassChunks, grounds: &Grounds) {
    let extent = Vec2::from_array(grounds.half_size) + Vec2::splat(grounds.settings.margin + 20.0);
    let count = (extent / TERRAIN_GRASS_CHUNK_SIZE).ceil().as_ivec2();
    for z in -count.y..count.y {
        for x in -count.x..count.x {
            let cell = IVec2::new(x, z);
            let center = (cell.as_vec2() + Vec2::splat(0.5)) * TERRAIN_GRASS_CHUNK_SIZE;
            if center.x.abs() + TERRAIN_GRASS_CHUNK_SIZE * 0.5 < grounds.half_size[0]
                && center.y.abs() + TERRAIN_GRASS_CHUNK_SIZE * 0.5 < grounds.half_size[1]
            {
                continue;
            }
            let origin = Vec3::new(center.x, grounds.height(center.x, center.y), center.y);
            let patch = GrassPatch {
                x1: cell.x as f32 * TERRAIN_GRASS_CHUNK_SIZE,
                x2: (cell.x + 1) as f32 * TERRAIN_GRASS_CHUNK_SIZE,
                z1: cell.y as f32 * TERRAIN_GRASS_CHUNK_SIZE,
                z2: (cell.y + 1) as f32 * TERRAIN_GRASS_CHUNK_SIZE,
                y: origin.y,
                level: grounds.settings.level,
                carrier: CarrierId::WORLD,
            };
            chunks.register(
                ChunkKey {
                    kind: ChunkKind::Grounds,
                    carrier: CarrierId::WORLD,
                    level: grounds.settings.level,
                    x,
                    z,
                },
                ChunkEntry {
                    patches: vec![patch],
                    source: GrassChunkSource::Grounds {
                        grounds: grounds.clone(),
                        cell,
                    },
                    origin,
                    carrier: CarrierId::WORLD,
                    parent: None,
                    level: None,
                },
            );
        }
    }
}
