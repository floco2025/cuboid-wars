use bevy::prelude::*;
use rand::{RngExt, rng};

use super::helpers::{load_repeating_texture, load_repeating_texture_linear, tiled_cuboid};
use crate::{constants::*, markers::*};
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
    wall: &Wall,
    debug_colors: bool,
) {
    // Calculate wall center and dimensions from corners
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X (visible faces will be the ±Z quads after rotation), width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    // Create material based on whether debug colors are enabled
    let wall_material = if debug_colors {
        let mut rng = rng();
        StandardMaterial {
            base_color: Color::srgb(
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
            ),
            ..default()
        }
    } else {
        StandardMaterial {
            base_color_texture: Some(load_repeating_texture(asset_server, "textures/wall/albedo.png")),
            normal_map_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/wall/normal-dx.png",
            )),
            occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/wall/ao.png")),
            metallic_roughness_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/wall/metallic-roughness.png",
            )),
            perceptual_roughness: TEXTURE_WALL_ROUGHNESS,
            metallic: TEXTURE_WALL_METALLIC,
            ..default()
        }
    };

    let mut mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, TEXTURE_WALL_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn(WallBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(wall_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT / 2.0, // Lift so bottom is at y=0
            center_z,
        )
        .with_rotation(rotation),
        visibility: Visibility::default(),
        marker: WallMarker,
    });
}
