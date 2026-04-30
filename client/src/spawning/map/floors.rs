use bevy::prelude::*;
use rand::{RngExt, rng};

use super::{
    helpers::{tiled_cuboid, tiled_floor_surface_meshes},
    materials::MapMaterialCache,
};
use crate::{config::AssetSet, markers::*};
use common::protocol::*;

// Spawn a visual cuboid slab for a `Floor`. Level-0 floors get the ground
// texture and a `GroundMarker`; higher levels get the roof texture and a
// `RoofMarker` so the R key / top-down view can hide them. The slab is
// `floor.thickness` deep, centered just below `floor.y` so the standing
// surface is at `floor.y`.
pub fn spawn_floor(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_cache: &mut MapMaterialCache,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    floor: &Floor,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let size_x = (floor.x2 - floor.x1).abs();
    let size_z = (floor.z2 - floor.z1).abs();
    let transform = Transform::from_xyz(center_x, floor.y - floor.thickness / 2.0, center_z);
    let material_ids = asset_set.material_ids_for_floor(floor);

    if debug_colors || material_ids.is_uniform() {
        let material_def = asset_set.material_by_id(material_ids.first());
        let material = if debug_colors {
            materials.add(random_debug_material())
        } else {
            material_cache.standard(material_ids.first(), material_def, asset_server, materials)
        };

        let mesh = tiled_cuboid(size_x, floor.thickness, size_z, material_def.tile_size());
        spawn_floor_mesh(commands, meshes, material, mesh, transform, floor.level);
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
        north_material_def.tile_size(),
        south_material_def.tile_size(),
        east_material_def.tile_size(),
        west_material_def.tile_size(),
        top_material_def.tile_size(),
        bottom_material_def.tile_size(),
    );

    let north_material = material_cache.standard(&material_ids.north, north_material_def, asset_server, materials);
    let south_material = material_cache.standard(&material_ids.south, south_material_def, asset_server, materials);
    let east_material = material_cache.standard(&material_ids.east, east_material_def, asset_server, materials);
    let west_material = material_cache.standard(&material_ids.west, west_material_def, asset_server, materials);
    let top_material = material_cache.standard(&material_ids.top, top_material_def, asset_server, materials);
    let bottom_material = material_cache.standard(&material_ids.bottom, bottom_material_def, asset_server, materials);

    spawn_floor_mesh(
        commands,
        meshes,
        north_material,
        surface_meshes.north,
        transform,
        floor.level,
    );
    spawn_floor_mesh(
        commands,
        meshes,
        south_material,
        surface_meshes.south,
        transform,
        floor.level,
    );
    spawn_floor_mesh(
        commands,
        meshes,
        east_material,
        surface_meshes.east,
        transform,
        floor.level,
    );
    spawn_floor_mesh(
        commands,
        meshes,
        west_material,
        surface_meshes.west,
        transform,
        floor.level,
    );
    spawn_floor_mesh(
        commands,
        meshes,
        top_material,
        surface_meshes.up,
        transform,
        floor.level,
    );
    spawn_floor_mesh(
        commands,
        meshes,
        bottom_material,
        surface_meshes.down,
        transform,
        floor.level,
    );
}

fn spawn_floor_mesh(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: Handle<StandardMaterial>,
    mut mesh: Mesh,
    transform: Transform,
    level: u8,
) {
    let _ = mesh.generate_tangents();
    if level == 0 {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            transform,
            Visibility::Visible,
            GroundMarker,
            MapLevel(level),
        ));
    } else {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            transform,
            Visibility::Visible,
            RoofMarker,
            MapLevel(level),
        ));
    }
}

fn random_debug_material() -> StandardMaterial {
    let mut rng = rng();
    StandardMaterial {
        base_color: Color::srgb(
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
        ),
        ..default()
    }
}
