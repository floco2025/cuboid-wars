use bevy::{math::Affine2, prelude::*};
use rand::{RngExt, rng};

use super::helpers::{load_repeating_texture, load_repeating_texture_linear, tiled_cuboid};
use crate::{constants::*, markers::*};
use common::protocol::*;

// Spawn a visual slab for a Floor. Level-0 floors render as flat planes with
// ground textures; higher levels render as cuboid slabs of `floor.thickness`
// with roof textures and a `RoofMarker` so the R key / top-down view can toggle
// them.
pub fn spawn_floor(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    floor: &Floor,
    debug_colors: bool,
) {
    if floor.level == 0 {
        spawn_ground(commands, meshes, materials, asset_server, floor, debug_colors);
    } else {
        spawn_upper(commands, meshes, materials, asset_server, floor, debug_colors);
    }
}

fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    floor: &Floor,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let width = (floor.x2 - floor.x1).abs();
    let depth = (floor.z2 - floor.z1).abs();

    let material = if debug_colors {
        StandardMaterial {
            base_color: Color::srgb(0.4, 0.4, 0.4),
            ..default()
        }
    } else {
        let uv_scale = Vec2::new(width / TEXTURE_FLOOR_TILE_SIZE, depth / TEXTURE_FLOOR_TILE_SIZE);
        StandardMaterial {
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
            uv_transform: Affine2::from_scale(uv_scale),
            perceptual_roughness: TEXTURE_FLOOR_ROUGHNESS,
            metallic: TEXTURE_FLOOR_METALLIC,
            ..default()
        }
    };

    let mut mesh = Mesh::from(Plane3d::default().mesh().size(width, depth));
    let _ = mesh.generate_tangents();

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        Transform::from_xyz(center_x, floor.y, center_z),
        Visibility::default(),
        GroundMarker,
    ));
}

fn spawn_upper(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    floor: &Floor,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let width = (floor.x2 - floor.x1).abs();
    let depth = (floor.z2 - floor.z1).abs();

    let material = if debug_colors {
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

    let mut mesh = tiled_cuboid(width, floor.thickness, depth, TEXTURE_ROOF_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        Transform::from_xyz(center_x, floor.y - floor.thickness / 2.0, center_z),
        Visibility::Visible,
        RoofMarker,
    ));
}
