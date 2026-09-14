use super::mesh::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_patch_mesh};
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
use common::protocol::{CarrierId, Floor, MapLayout, TERRAIN_MATERIAL};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Component)]
pub struct TerrainMarker;

#[derive(Component, Clone)]
pub struct GrassChunkVisual {
    pub(super) patches: Vec<GrassPatch>,
    pub(super) footprint: Arc<[Floor]>,
    pub(super) lod: GrassLod,
    pub(super) origin: Vec3,
    pub(super) green: Color,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GrassPatch {
    pub(super) x1: f32,
    pub(super) x2: f32,
    pub(super) z1: f32,
    pub(super) z2: f32,
    pub(super) y: f32,
    pub(super) level: u8,
    pub(super) carrier: CarrierId,
}

impl GrassPatch {
    fn clipped_to_chunk(floor: Floor, chunk_x: i32, chunk_z: i32) -> Option<Self> {
        let (floor_x1, floor_x2, floor_z1, floor_z2) = floor.bounds_xz();
        let chunk_x1 = chunk_x as f32 * TERRAIN_GRASS_CHUNK_SIZE;
        let chunk_z1 = chunk_z as f32 * TERRAIN_GRASS_CHUNK_SIZE;
        let x1 = floor_x1.max(chunk_x1);
        let x2 = floor_x2.min(chunk_x1 + TERRAIN_GRASS_CHUNK_SIZE);
        let z1 = floor_z1.max(chunk_z1);
        let z2 = floor_z2.min(chunk_z1 + TERRAIN_GRASS_CHUNK_SIZE);
        (x2 > x1 && z2 > z1).then_some(Self {
            x1,
            x2,
            z1,
            z2,
            y: floor.y,
            level: floor.level,
            carrier: floor.carrier,
        })
    }

    pub(super) fn area(self) -> f32 {
        (self.x2 - self.x1) * (self.z2 - self.z1)
    }

    #[cfg(test)]
    pub(super) fn contains_base(self, x: f32, z: f32) -> bool {
        const EPSILON: f32 = 0.0001;
        x >= self.x1 - EPSILON && x <= self.x2 + EPSILON && z >= self.z1 - EPSILON && z <= self.z2 + EPSILON
    }
}

type ChunkKey = (CarrierId, u8, i32, i32);

// Terrain cells are floor slabs with authored sides and bottoms. The ordinary
// geometry batch omits their procedural top faces; this system supplies the
// exact compiled floor footprints (including trim), and vegetation follows
// those same footprints while batching by ten-metre chunks.
#[allow(clippy::too_many_arguments)]
pub fn terrain_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
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
        client_settings.grass.base_color(),
    ));
    let grass_material = grass_materials.add(grass_material());
    let terrain_floors = layout
        .floors
        .iter()
        .zip(layout.floor_materials.iter())
        .filter_map(|(floor, materials)| (materials.top == TERRAIN_MATERIAL).then_some(*floor))
        .collect::<Vec<_>>();
    let mut surface_groups: BTreeMap<(CarrierId, u8), Vec<Floor>> = BTreeMap::new();
    for floor in terrain_floors.iter().copied() {
        surface_groups
            .entry((floor.carrier, floor.level))
            .or_default()
            .push(floor);
    }
    for ((carrier, level), floors) in surface_groups {
        let (min_x, max_x, min_z, max_z) = floors.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY),
            |(min_x, max_x, min_z, max_z), floor| {
                let (x1, x2, z1, z2) = floor.bounds_xz();
                (min_x.min(x1), max_x.max(x2), min_z.min(z1), max_z.max(z2))
            },
        );
        let origin = Vec3::new(f32::midpoint(min_x, max_x), floors[0].y, f32::midpoint(min_z, max_z));
        commands.spawn((
            TerrainMarker,
            storeys.tag(carrier, level, 0),
            ChildOf(carrier_entities.get(carrier)),
            Mesh3d(meshes.add(terrain_surface_mesh(&floors, origin))),
            MeshMaterial3d(surface_material.clone()),
            Transform::from_translation(origin),
            Visibility::Visible,
            NotShadowCaster,
        ));
    }

    let footprint: Arc<[Floor]> = terrain_floors.into();
    let mut chunks: BTreeMap<ChunkKey, Vec<GrassPatch>> = BTreeMap::new();
    for floor in footprint.iter().copied() {
        let (x1, x2, z1, z2) = floor.bounds_xz();
        let min_chunk_x = (x1 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_x = (x2 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let min_chunk_z = (z1 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_z = (z2 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        for chunk_z in min_chunk_z..=max_chunk_z {
            for chunk_x in min_chunk_x..=max_chunk_x {
                let Some(patch) = GrassPatch::clipped_to_chunk(floor, chunk_x, chunk_z) else {
                    continue;
                };
                chunks
                    .entry((floor.carrier, floor.level, chunk_x, chunk_z))
                    .or_default()
                    .push(patch);
            }
        }
    }

    for ((carrier, level, chunk_x, chunk_z), patches) in chunks {
        let origin = Vec3::new(
            (chunk_x as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
            patches[0].y,
            (chunk_z as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
        );
        if !client_settings.grass.enabled {
            continue;
        }
        let level_tag = storeys.tag(carrier, level, 0);
        for (lod, range) in [
            (GrassLod::Near, TERRAIN_GRASS_NEAR_RANGE),
            (GrassLod::Mid, TERRAIN_GRASS_MID_RANGE),
        ] {
            let green = client_settings.grass.base_color();
            let Some(mesh) = grass_chunk_mesh(&patches, &footprint, lod, origin, green, &[]) else {
                continue;
            };
            let mut bounds = mesh.compute_aabb().expect("terrain grass positions missing");
            let sway = GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
            bounds.half_extents.x += sway;
            bounds.half_extents.z += sway;
            commands.spawn((
                TerrainMarker,
                GrassChunkVisual {
                    patches: patches.clone(),
                    footprint: footprint.clone(),
                    lod,
                    origin,
                    green,
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

pub(in crate::map) fn grass_material() -> GrassMaterial {
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

fn terrain_surface_mesh(floors: &[Floor], origin: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(floors.len() * 4);
    let mut normals = Vec::with_capacity(floors.len() * 4);
    let mut uvs = Vec::with_capacity(floors.len() * 4);
    let mut indices = Vec::with_capacity(floors.len() * 6);
    for floor in floors {
        let base = positions.len() as u32;
        let (x1, x2, z1, z2) = floor.bounds_xz();
        for (x, z) in [(x1, z1), (x2, z1), (x1, z2), (x2, z2)] {
            positions.push((Vec3::new(x, floor.y, z) - origin).to_array());
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
    patches: &[GrassPatch],
    footprint: &[Floor],
    lod: GrassLod,
    origin: Vec3,
    green: Color,
    burns: &[super::burn::GrassBurn],
) -> Option<Mesh> {
    let mut merged: Option<Mesh> = None;
    for &patch in patches {
        let mesh = grass_patch_mesh(patch, footprint, lod, green, burns);
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

#[cfg(test)]
pub(super) fn terrain_patch_aabb(patch: GrassPatch) -> Aabb {
    let pad = BLADE_MAX_OVERHANG + GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    Aabb::from_min_max(
        Vec3::new(patch.x1 - pad, patch.y, patch.z1 - pad),
        Vec3::new(patch.x2 + pad, patch.y + BLADE_HEIGHT_MAX, patch.z2 + pad),
    )
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn procedural_surface_uses_compiled_floor_bounds_including_trim() {
        let floor = Floor {
            x1: -1.25,
            z1: -1.0,
            x2: 1.4,
            z2: 1.3,
            y: 2.0,
            thickness: 0.2,
            level: 1,
            carrier: CarrierId::WORLD,
        };
        let mesh = terrain_surface_mesh(&[floor], Vec3::new(0.0, 2.0, 0.0));
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("terrain surface positions missing");
        };
        assert_eq!(
            positions,
            &[[-1.25, 0.0, -1.0], [1.4, 0.0, -1.0], [-1.25, 0.0, 1.3], [1.4, 0.0, 1.3]]
        );
    }
}
