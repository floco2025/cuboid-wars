use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    camera::{primitives::MeshAabb, visibility::VisibilityRange},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::map::Grounds;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use super::{grass::GrassLod, grounds::GroundsVisual, terrain_surface::TerrainCover};
use crate::{
    constants::{
        GRASS_WIND_DIRECTION_DEGREES, GRASS_WIND_SPEED, GRASS_WIND_STRENGTH, TERRAIN_GRASS_CHUNK_SIZE,
        TERRAIN_GRASS_DRY, TERRAIN_GRASS_GREEN, TERRAIN_GRASS_MID_DENSITY, TERRAIN_GRASS_MID_RANGE,
        TERRAIN_GRASS_NEAR_DENSITY, TERRAIN_GRASS_NEAR_RANGE,
    },
    materials::{GrassMaterial, GrassWindExtension},
};

pub(super) fn spawn_terrain_grass(
    commands: &mut Commands,
    grounds: &Grounds,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<GrassMaterial>,
) {
    let direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
    let material = materials.add(GrassMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            cull_mode: None,
            ..default()
        },
        extension: GrassWindExtension {
            wind: Vec4::new(direction.x, direction.y, GRASS_WIND_STRENGTH * 0.5, GRASS_WIND_SPEED),
        },
    });
    let extent = Vec2::from_array(grounds.half_size) + Vec2::splat(grounds.settings.margin + 20.0);
    let count = (extent / TERRAIN_GRASS_CHUNK_SIZE).ceil().as_ivec2();
    for z in -count.y..count.y {
        for x in -count.x..count.x {
            let cell = IVec2::new(x, z);
            let center = (cell.as_vec2() + Vec2::splat(0.5)) * TERRAIN_GRASS_CHUNK_SIZE;
            if center.x.abs() + TERRAIN_GRASS_CHUNK_SIZE * 0.5 < grounds.half_size[0]
                && center.y.abs() + TERRAIN_GRASS_CHUNK_SIZE * 0.5 < grounds.half_size[1]
            {
                continue;
            }
            let origin = Vec3::new(center.x, grounds.height(center.x, center.y), center.y);
            for (lod, range) in [
                (GrassLod::Near, TERRAIN_GRASS_NEAR_RANGE),
                (GrassLod::Mid, TERRAIN_GRASS_MID_RANGE),
            ] {
                let Some(mesh) = grass_chunk(grounds, cell, origin, lod) else {
                    continue;
                };
                let mut bounds = mesh.compute_aabb().expect("terrain grass positions missing");
                bounds.half_extents.x += GRASS_WIND_STRENGTH * 0.7;
                bounds.half_extents.z += GRASS_WIND_STRENGTH * 0.7;
                commands.spawn((
                    GroundsVisual,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(origin),
                    NotShadowCaster,
                    bounds,
                    VisibilityRange {
                        start_margin: range[0]..range[1],
                        end_margin: range[2]..range[3],
                        use_aabb: false,
                    },
                ));
            }
        }
    }
}

fn grass_chunk(grounds: &Grounds, cell: IVec2, origin: Vec3, lod: GrassLod) -> Option<Mesh> {
    let seed = (cell.x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (cell.y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let (density, blades) = match lod {
        GrassLod::Near => (TERRAIN_GRASS_NEAR_DENSITY, 3),
        GrassLod::Mid => (TERRAIN_GRASS_MID_DENSITY, 2),
    };
    let count = (TERRAIN_GRASS_CHUNK_SIZE.powi(2) * density) as usize;
    for _ in 0..count {
        let x = (cell.x as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
        let z = (cell.y as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
        if grounds.distance_outside_map(x, z) < 0.2 {
            continue;
        }
        let cover = TerrainCover::at(Vec2::new(x, z));
        if rng.random::<f32>() > cover.grass_density(Vec2::new(x, z)) {
            continue;
        }
        let root = Vec3::new(x, grounds.height(x, z) - 0.025, z) - origin;
        let tint = TERRAIN_GRASS_GREEN
            .to_linear()
            .mix(&TERRAIN_GRASS_DRY.to_linear(), cover.dry);
        let phase = rng.random::<f32>();
        for _ in 0..blades {
            let direction = Vec2::from_angle(rng.random_range(0.0..TAU));
            let across = Vec3::new(direction.x, 0.0, direction.y) * rng.random_range(0.005..0.011);
            let lean = Vec3::new(-direction.y, 0.0, direction.x) * rng.random_range(0.06..0.16);
            let height = rng.random_range(0.10..0.23);
            let base = positions.len() as u32;
            let mid = root + Vec3::Y * height * 0.65 + lean * 0.3;
            let tip = root + Vec3::Y * height + lean;
            for (pos, weight, light) in [
                (root - across, 0.0, 0.45),
                (root + across, 0.0, 0.45),
                (mid - across * 0.6, 0.55, 0.85),
                (mid + across * 0.6, 0.55, 0.85),
                (tip, 1.0, 1.12),
            ] {
                positions.push(pos.to_array());
                uvs.push([weight, phase]);
                colors.push([
                    tint.red * light * cover.shade,
                    tint.green * light * cover.shade,
                    tint.blue * light * cover.shade,
                    1.0,
                ]);
            }
            indices.extend([0, 1, 3, 0, 3, 2, 2, 3, 4].map(|i| base + i));
        }
    }
    if positions.is_empty() {
        return None;
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_indices(Indices::U32(indices)),
    )
}
