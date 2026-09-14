use bevy::{
    camera::{primitives::MeshAabb, visibility::VisibilityRange},
    light::NotShadowCaster,
    prelude::*,
};
use common::map::Grounds;
use rand::RngExt;

use super::{
    grass::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_material, grass_scatter_mesh},
    grounds::GroundsVisual,
};
use crate::{
    constants::{GRASS_WIND_STRENGTH, TERRAIN_GRASS_CHUNK_SIZE, TERRAIN_GRASS_MID_RANGE, TERRAIN_GRASS_NEAR_RANGE},
    materials::GrassMaterial,
};

pub(super) fn spawn_terrain_grass(
    commands: &mut Commands,
    grounds: &Grounds,
    green: Color,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<GrassMaterial>,
) {
    let material = materials.add(grass_material());
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
                let Some(mesh) = grass_chunk(grounds, cell, origin, lod, green) else {
                    continue;
                };
                let mut bounds = mesh.compute_aabb().expect("terrain grass positions missing");
                let sway = GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
                bounds.half_extents.x += sway;
                bounds.half_extents.z += sway;
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

fn grass_chunk(grounds: &Grounds, cell: IVec2, origin: Vec3, lod: GrassLod, green: Color) -> Option<Mesh> {
    let seed = (cell.x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (cell.y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    let mesh = grass_scatter_mesh(
        seed,
        lod.tuft_count(TERRAIN_GRASS_CHUNK_SIZE.powi(2)),
        lod,
        green,
        &[],
        |rng| {
            let x = (cell.x as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
            let z = (cell.y as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
            (grounds.distance_outside_map(x, z) >= 0.2).then(|| Vec3::new(x, grounds.height(x, z), z))
        },
        |_, _| true,
    );
    if mesh.count_vertices() == 0 {
        return None;
    }
    Some(mesh.transformed_by(Transform::from_translation(-origin)))
}
