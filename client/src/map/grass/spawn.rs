use super::mesh::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_cell_mesh};
#[cfg(test)]
use super::mesh::{BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
    constants::{
        GRASS_WIND_DIRECTION_DEGREES, GRASS_WIND_SPEED, GRASS_WIND_STRENGTH, TERRAIN_GRASS_CHUNK_SIZE,
        TERRAIN_GRASS_MID_RANGE, TERRAIN_GRASS_NEAR_RANGE,
    },
    map::{DebugColorMode, DebugColors},
    materials::{GrassMaterial, GrassWindExtension, TerrainMaterial, terrain_material},
};
#[cfg(test)]
use bevy::camera::primitives::Aabb;
use bevy::{
    asset::RenderAssetUsages,
    camera::{primitives::MeshAabb, visibility::VisibilityRange},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::protocol::{CarrierId, MapLayout, MapSettings, TerrainCell};
use std::collections::{BTreeMap, HashSet};

#[derive(Component)]
pub struct TerrainMarker;

#[derive(Component, Clone)]
pub struct GrassChunkVisual {
    pub(super) cells: Vec<(TerrainCell, OpenEdges)>,
    pub(super) lod: GrassLod,
    pub(super) origin: Vec3,
}

#[derive(Clone, Copy)]
pub(super) struct OpenEdges {
    pub(super) pos_x: bool,
    pub(super) neg_x: bool,
    pub(super) pos_z: bool,
    pub(super) neg_z: bool,
}

impl OpenEdges {
    fn for_cell(cell: TerrainCell, cell_size: f32, painted: &HashSet<TerrainKey>) -> Self {
        let (carrier, x, z, level) = quantized_key(cell, cell_size);
        Self {
            pos_x: painted.contains(&(carrier, x + 2, z, level)),
            neg_x: painted.contains(&(carrier, x - 2, z, level)),
            pos_z: painted.contains(&(carrier, x, z + 2, level)),
            neg_z: painted.contains(&(carrier, x, z - 2, level)),
        }
    }
}

type ChunkKey = (CarrierId, u8, i32, i32);

// Terrain cells are floor slabs with authored sides and bottoms. The ordinary
// geometry batch omits their procedural top faces; this system supplies those
// tops and batches vegetation by ten-metre chunks.
#[allow(clippy::too_many_arguments)]
pub fn terrain_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    map_settings: Res<MapSettings>,
    client_settings: Res<ClientSettings>,
    server: Res<AssetServer>,
    debug_colors: Res<DebugColors>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    existing: Query<Entity, With<TerrainMarker>>,
    mut last_mode: Local<Option<DebugColorMode>>,
) {
    if !layout.is_changed() && last_mode.as_ref() == Some(&debug_colors.0) {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    *last_mode = Some(debug_colors.0);
    if debug_colors.0 != DebugColorMode::Off || layout.terrain.is_empty() {
        return;
    }

    let surface_material = terrain_materials.add(terrain_material(
        &server,
        client_settings.rendering.texture_anisotropy,
        client_settings.rendering.mipmaps,
    ));
    let grass_material = grass_materials.add(grass_material());
    let cell_size = map_settings.geometry.grid_cell_size;
    let painted: HashSet<TerrainKey> = layout
        .terrain
        .iter()
        .map(|cell| quantized_key(*cell, cell_size))
        .collect();
    let mut chunks: BTreeMap<ChunkKey, Vec<(TerrainCell, OpenEdges)>> = BTreeMap::new();
    for cell in layout.terrain.iter().copied() {
        let chunk_x = (cell.x / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let chunk_z = (cell.z / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        chunks
            .entry((cell.carrier, cell.level, chunk_x, chunk_z))
            .or_default()
            .push((cell, OpenEdges::for_cell(cell, cell_size, &painted)));
    }

    for ((carrier, level, chunk_x, chunk_z), cells) in chunks {
        let origin = Vec3::new(
            (chunk_x as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
            cells[0].0.y,
            (chunk_z as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
        );
        let level_tag = storeys.tag(carrier, level, 0);
        commands.spawn((
            TerrainMarker,
            level_tag,
            ChildOf(carrier_entities.get(carrier)),
            Mesh3d(meshes.add(terrain_surface_mesh(&cells, cell_size, origin))),
            MeshMaterial3d(surface_material.clone()),
            Transform::from_translation(origin),
            Visibility::Visible,
            NotShadowCaster,
        ));

        if !client_settings.grass.enabled {
            continue;
        }
        for (lod, range) in [
            (GrassLod::Near, TERRAIN_GRASS_NEAR_RANGE),
            (GrassLod::Mid, TERRAIN_GRASS_MID_RANGE),
        ] {
            let Some(mesh) = grass_chunk_mesh(&cells, cell_size, lod, origin, &[]) else {
                continue;
            };
            let mut bounds = mesh.compute_aabb().expect("terrain grass positions missing");
            let sway = GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
            bounds.half_extents.x += sway;
            bounds.half_extents.z += sway;
            commands.spawn((
                TerrainMarker,
                GrassChunkVisual {
                    cells: cells.clone(),
                    lod,
                    origin,
                },
                level_tag,
                ChildOf(carrier_entities.get(carrier)),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(grass_material.clone()),
                Transform::from_translation(origin),
                Visibility::Visible,
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

fn grass_material() -> GrassMaterial {
    let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
    GrassMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            cull_mode: None,
            ..default()
        },
        extension: GrassWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                GRASS_WIND_STRENGTH,
                GRASS_WIND_SPEED,
            ),
        },
    }
}

fn terrain_surface_mesh(cells: &[(TerrainCell, OpenEdges)], cell_size: f32, origin: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(cells.len() * 4);
    let mut normals = Vec::with_capacity(cells.len() * 4);
    let mut uvs = Vec::with_capacity(cells.len() * 4);
    let mut indices = Vec::with_capacity(cells.len() * 6);
    let half = cell_size * 0.5;
    for &(cell, _) in cells {
        let base = positions.len() as u32;
        for (x, z) in [
            (cell.x - half, cell.z - half),
            (cell.x + half, cell.z - half),
            (cell.x - half, cell.z + half),
            (cell.x + half, cell.z + half),
        ] {
            positions.push((Vec3::new(x, cell.y, z) - origin).to_array());
            normals.push(Vec3::Y.to_array());
            // Carrier-local metres keep carried terrain stable. World-carrier
            // coordinates are exactly those used by exterior ground.
            uvs.push([x, z]);
        }
        indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

pub(super) fn grass_chunk_mesh(
    cells: &[(TerrainCell, OpenEdges)],
    cell_size: f32,
    lod: GrassLod,
    origin: Vec3,
    burns: &[super::burn::GrassBurn],
) -> Option<Mesh> {
    let mut merged: Option<Mesh> = None;
    for &(cell, open) in cells {
        let mesh = grass_cell_mesh(cell, cell_size, lod, open, burns);
        if mesh.count_vertices() == 0 {
            continue;
        }
        match &mut merged {
            Some(chunk) => chunk
                .merge(&mesh)
                .expect("terrain grass cells have incompatible vertex layouts"),
            None => merged = Some(mesh),
        }
    }
    merged.map(|mesh| mesh.transformed_by(Transform::from_translation(-origin)))
}

pub(super) type TerrainKey = (CarrierId, i64, i64, u8);

pub(super) fn quantized_key(cell: TerrainCell, cell_size: f32) -> TerrainKey {
    let quantized_x = (cell.x * 2.0 / cell_size).round() as i64;
    let quantized_z = (cell.z * 2.0 / cell_size).round() as i64;
    (cell.carrier, quantized_x, quantized_z, cell.level)
}

#[cfg(test)]
pub(super) fn terrain_cell_aabb(cell: TerrainCell, cell_size: f32) -> Aabb {
    let pad = cell_size / 2.0 + BLADE_MAX_OVERHANG + GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    Aabb::from_min_max(
        Vec3::new(cell.x - pad, cell.y, cell.z - pad),
        Vec3::new(cell.x + pad, cell.y + BLADE_HEIGHT_MAX, cell.z + pad),
    )
}
