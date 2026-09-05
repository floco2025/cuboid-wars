use bevy::prelude::*;

use crate::{
    config::{AssetSet, ClientSettings},
    map::{MapLevel, tiled_cuboid, tiled_floor_surface_meshes},
    materials::MaterialHandleCache,
};
use common::{
    map::surface_center_at,
    protocol::{FaceMaterials, MapLayout, MovingFloor},
};

// Marks a tile's root entity; `index` is its slot in `MapLayout.moving_floors`
// (and `MovingFloors`), `levels` the storeys it spans above its `MapLevel`,
// so level focus shows it from every storey it passes through.
#[derive(Component)]
pub struct MovingFloorMarker {
    pub index: usize,
    pub levels: u8,
}

// One root entity per `MovingFloor`, placed at the tile's surface center and
// moved every frame by `moving_floors_transform_sync_system`; the face
// meshes hang below it as children. UVs are mesh-local rather than
// world-anchored like every static map mesh: the tile moves, and a texture
// projected from world position would swim across it.
pub fn moving_floors_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut material_cache: Local<MaterialHandleCache>,
    existing: Query<Entity, With<MovingFloorMarker>>,
) {
    if !map_layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    *material_cache = MaterialHandleCache::default();

    for (index, (floor, face_materials)) in map_layout
        .moving_floors
        .iter()
        .zip(&map_layout.moving_floor_materials)
        .enumerate()
    {
        let mut material = |id: &str| {
            material_cache.standard(
                id,
                asset_set.material_by_id(id),
                &asset_server,
                &mut materials,
                client_settings.rendering.texture_anisotropy,
                client_settings.rendering.mipmaps,
            )
        };
        let faces = tile_faces(&asset_set, floor, face_materials);
        let start = surface_center_at(floor, 0);
        commands
            .spawn((
                MovingFloorMarker {
                    index,
                    levels: floor.levels,
                },
                MapLevel(floor.level),
                Transform::from_translation(start),
                Visibility::Visible,
            ))
            .with_children(|tile| {
                for (material_id, mut mesh) in faces {
                    let _ = mesh.generate_tangents();
                    tile.spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(material(&material_id)),
                        Transform::from_translation(Vec3::NEG_Y * (floor.thickness / 2.0)),
                    ));
                }
            });
    }
}

// The tile's faces with their material ids: one cuboid when every face
// shares a material, six faces otherwise, all centered on the origin.
fn tile_faces(asset_set: &AssetSet, floor: &MovingFloor, materials: &FaceMaterials) -> Vec<(String, Mesh)> {
    let size_x = floor.half_x * 2.0;
    let size_z = floor.half_z * 2.0;
    let tile_size = |id: &str| asset_set.material_by_id(id).tile_size();
    if materials.is_uniform() {
        let id = materials.primary();
        let mesh = tiled_cuboid(
            size_x,
            floor.thickness,
            size_z,
            tile_size(id),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        return vec![(id.to_owned(), mesh)];
    }
    let faces = tiled_floor_surface_meshes(
        size_x,
        floor.thickness,
        size_z,
        Vec3::ZERO,
        tile_size(&materials.north),
        tile_size(&materials.south),
        tile_size(&materials.east),
        tile_size(&materials.west),
        tile_size(&materials.top),
        tile_size(&materials.bottom),
    );
    vec![
        (materials.north.clone(), faces.north),
        (materials.south.clone(), faces.south),
        (materials.east.clone(), faces.east),
        (materials.west.clone(), faces.west),
        (materials.top.clone(), faces.up),
        (materials.bottom.clone(), faces.down),
    ]
}
