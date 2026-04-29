use bevy::prelude::*;
use rand::{RngExt, rng};

use super::helpers::tiled_cuboid;
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
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    floor: &Floor,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let material_def = asset_set.material_for_floor(floor);
    let material = if debug_colors {
        random_debug_material()
    } else {
        material_def.standard_material(asset_server)
    };

    let mut mesh = tiled_cuboid(
        (floor.x2 - floor.x1).abs(),
        floor.thickness,
        (floor.z2 - floor.z1).abs(),
        material_def.tile_size(),
    );
    let _ = mesh.generate_tangents();
    let transform = Transform::from_xyz(center_x, floor.y - floor.thickness / 2.0, center_z);

    if floor.level == 0 {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(material)),
            transform,
            Visibility::Visible,
            GroundMarker,
            MapLevel(floor.level),
        ));
    } else {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(material)),
            transform,
            Visibility::Visible,
            RoofMarker,
            MapLevel(floor.level),
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
