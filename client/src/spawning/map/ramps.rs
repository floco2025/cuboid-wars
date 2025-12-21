use bevy::prelude::*;

use crate::{constants::*, markers::*};
use common::protocol::*;

use super::helpers::{build_ramp_meshes, load_repeating_texture, load_repeating_texture_linear};

#[derive(Bundle)]
struct RampBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RampMarker,
}

// Spawn a ramp entity based on shared `Ramp` config.
pub fn spawn_ramp(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    ramp: &Ramp,
) {
    // Build meshes split by material usage
    let (mesh_top, mesh_side) = build_ramp_meshes(ramp.x1, ramp.z1, ramp.x2, ramp.z2, ramp.y1, ramp.y2);

    // Floor material for the ramp top
    let mut top_material = StandardMaterial {
        base_color_texture: Some(load_repeating_texture(asset_server, "textures/ground/albedo.png")),
        normal_map_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/ground/normal-dx.png",
        )),
        occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/ground/ao.png")),
        metallic_roughness_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/ground/metallic-roughness.png",
        )),
        perceptual_roughness: TEXTURE_FLOOR_ROUGHNESS,
        metallic: TEXTURE_FLOOR_METALLIC,
        ..default()
    };
    top_material.alpha_mode = AlphaMode::Opaque;
    top_material.base_color.set_alpha(1.0);

    // Wall material for the ramp sides
    let mut side_material = StandardMaterial {
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
    };
    side_material.alpha_mode = AlphaMode::Opaque;
    side_material.base_color.set_alpha(1.0);

    // Top entity (floor texture)
    commands.spawn(RampBundle {
        mesh: Mesh3d(meshes.add(mesh_top)),
        material: MeshMaterial3d(materials.add(top_material)),
        transform: Transform::default(),
        visibility: Visibility::Visible,
        marker: RampMarker,
    });

    // Side entity (wall texture)
    commands.spawn(RampBundle {
        mesh: Mesh3d(meshes.add(mesh_side)),
        material: MeshMaterial3d(materials.add(side_material)),
        transform: Transform::default(),
        visibility: Visibility::Visible,
        marker: RampMarker,
    });
}
