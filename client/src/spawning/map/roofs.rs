use bevy::prelude::*;

use super::helpers::{load_repeating_texture, load_repeating_texture_linear, tiled_cuboid};
use crate::{constants::*, markers::*};
use common::{constants::*, protocol::*};

#[derive(Bundle)]
struct RoofBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RoofMarker,
}

// Spawn a roof entity based on a shared `Roof` config.
pub fn spawn_roof(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    roof: &Roof,
    debug_colors: bool,
) {
    use rand::Rng;

    // Calculate roof center and dimensions from corners
    let center_x = f32::midpoint(roof.x1, roof.x2);
    let center_z = f32::midpoint(roof.z1, roof.z2);

    let width = (roof.x2 - roof.x1).abs();
    let depth = (roof.z2 - roof.z1).abs();

    // Create material based on whether debug colors are enabled
    let roof_material = if debug_colors {
        let mut rng = rand::rng();
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
            base_color_texture: Some(load_repeating_texture(asset_server, "textures/roof/albedo.png")),
            normal_map_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/roof/normal-dx.png",
            )),
            occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/roof/ao.png")),
            metallic_roughness_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/roof/metallic-roughness.png",
            )),
            perceptual_roughness: TEXTURE_ROOF_ROUGHNESS,
            metallic: TEXTURE_ROOF_METALLIC,
            ..default()
        }
    };

    // Use the actual aspect ratio to compute tile repeats for square texels
    let mut mesh = tiled_cuboid(width, roof.thickness, depth, TEXTURE_ROOF_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn(RoofBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(roof_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT + roof.thickness / 2.0, // Position so bottom of roof sits on top of wall
            center_z,
        ),
        visibility: Visibility::Visible,
        marker: RoofMarker,
    });
}
