use bevy::prelude::*;
use rand::{RngExt, rng};

use super::helpers::{load_repeating_texture, load_repeating_texture_linear, tiled_cuboid};
use crate::{constants::*, markers::*};
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
    floor: &Floor,
    debug_colors: bool,
) {
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let width = (floor.x2 - floor.x1).abs();
    let depth = (floor.z2 - floor.z1).abs();

    let is_ground = floor.level == 0;
    let (albedo, normal, ao, mr, tile_size, roughness, metallic) = if is_ground {
        (
            "textures/ground/albedo.png",
            "textures/ground/normal-dx.png",
            "textures/ground/ao.png",
            "textures/ground/metallic-roughness.png",
            TEXTURE_FLOOR_TILE_SIZE,
            TEXTURE_FLOOR_ROUGHNESS,
            TEXTURE_FLOOR_METALLIC,
        )
    } else {
        (
            "textures/roof/albedo.png",
            "textures/roof/normal-dx.png",
            "textures/roof/ao.png",
            "textures/roof/metallic-roughness.png",
            TEXTURE_ROOF_TILE_SIZE,
            TEXTURE_ROOF_ROUGHNESS,
            TEXTURE_ROOF_METALLIC,
        )
    };

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
            base_color_texture: Some(load_repeating_texture(asset_server, albedo)),
            normal_map_texture: Some(load_repeating_texture_linear(asset_server, normal)),
            occlusion_texture: Some(load_repeating_texture_linear(asset_server, ao)),
            metallic_roughness_texture: Some(load_repeating_texture_linear(asset_server, mr)),
            perceptual_roughness: roughness,
            metallic,
            ..default()
        }
    };

    let mut mesh = tiled_cuboid(width, floor.thickness, depth, tile_size);
    let _ = mesh.generate_tangents();

    let transform = Transform::from_xyz(center_x, floor.y - floor.thickness / 2.0, center_z);

    if is_ground {
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
