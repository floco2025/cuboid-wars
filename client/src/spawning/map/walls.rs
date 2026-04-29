use bevy::prelude::*;
use rand::{RngExt, rng};

use super::helpers::tiled_cuboid;
use crate::{config::AssetSet, markers::*};
use common::{constants::*, protocol::*};

#[derive(Bundle)]
struct WallBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: WallMarker,
}

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    wall: &Wall,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X (visible faces will be the ±Z quads after rotation), width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    let material_def = asset_set.material_for_wall(wall);
    let wall_material = if debug_colors {
        random_debug_material()
    } else {
        material_def.standard_material(asset_server)
    };

    let mut mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, material_def.tile_size());
    let _ = mesh.generate_tangents();

    let level_y = f32::from(wall.level) * LEVEL_HEIGHT;
    commands.spawn((
        WallBundle {
            mesh: Mesh3d(meshes.add(mesh)),
            material: MeshMaterial3d(materials.add(wall_material)),
            transform: Transform::from_xyz(center_x, level_y + WALL_HEIGHT / 2.0, center_z).with_rotation(rotation),
            visibility: Visibility::default(),
            marker: WallMarker,
        },
        MapLevel(wall.level),
    ));
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
