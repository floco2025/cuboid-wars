use bevy::prelude::*;

use super::{
    cuboid_mesh::{tiled_cuboid, tiled_floor_surface_meshes},
    geometry_batch::{MapGeometryBatch, MapGeometryKind},
};
use crate::config::AssetSet;
use common::{protocol::FaceMaterials, protocol::*};

// Spawn a visual cuboid slab for a `Floor`. Level-0 floors get the ground
// texture and a `GroundMarker`; higher levels get the roof texture and a
// `RoofMarker` so the R key / top-down view can hide them. The slab is
// `floor.thickness` deep, centered just below `floor.y` so the standing
// surface is at `floor.y`.
pub fn batch_floor(batcher: &mut MapGeometryBatch, asset_set: &AssetSet, floor: &Floor, material_ids: &FaceMaterials) {
    batcher.begin_segment();
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let center_y = floor.y - floor.thickness / 2.0;
    let size_x = (floor.x2 - floor.x1).abs();
    let size_z = (floor.z2 - floor.z1).abs();
    let world_center = Vec3::new(center_x, center_y, center_z);
    let transform = Transform::from_translation(world_center);
    let kind = if floor.level == 0 {
        MapGeometryKind::Ground
    } else {
        MapGeometryKind::Roof
    };

    if material_ids.is_uniform() {
        let material_def = asset_set.material_by_id(material_ids.primary());
        let mesh = tiled_cuboid(
            size_x,
            floor.thickness,
            size_z,
            material_def.tile_size(),
            world_center,
            Quat::IDENTITY,
        );
        batcher.add_mesh(kind, floor.level, material_ids.primary(), &mesh, transform);
        return;
    }

    let north_material_def = asset_set.material_by_id(&material_ids.north);
    let south_material_def = asset_set.material_by_id(&material_ids.south);
    let east_material_def = asset_set.material_by_id(&material_ids.east);
    let west_material_def = asset_set.material_by_id(&material_ids.west);
    let top_material_def = asset_set.material_by_id(&material_ids.top);
    let bottom_material_def = asset_set.material_by_id(&material_ids.bottom);
    let surface_meshes = tiled_floor_surface_meshes(
        size_x,
        floor.thickness,
        size_z,
        world_center,
        north_material_def.tile_size(),
        south_material_def.tile_size(),
        east_material_def.tile_size(),
        west_material_def.tile_size(),
        top_material_def.tile_size(),
        bottom_material_def.tile_size(),
    );

    batcher.add_mesh(kind, floor.level, &material_ids.north, &surface_meshes.north, transform);
    batcher.add_mesh(kind, floor.level, &material_ids.south, &surface_meshes.south, transform);
    batcher.add_mesh(kind, floor.level, &material_ids.east, &surface_meshes.east, transform);
    batcher.add_mesh(kind, floor.level, &material_ids.west, &surface_meshes.west, transform);
    batcher.add_mesh(kind, floor.level, &material_ids.top, &surface_meshes.up, transform);
    batcher.add_mesh(kind, floor.level, &material_ids.bottom, &surface_meshes.down, transform);
}
