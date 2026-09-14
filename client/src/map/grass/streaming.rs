use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    camera::{
        primitives::{Aabb, MeshAabb},
        visibility::VisibilityRange,
    },
    light::NotShadowCaster,
    prelude::*,
};
use common::{
    map::{Carriers, Grounds},
    protocol::{CarrierId, Floor, MapLayout, TERRAIN_MATERIAL},
};
use rand::RngExt;

use super::{
    burn::GrassBurn,
    mesh::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_patch_mesh, grass_scatter_mesh},
    spawn::{GrassPatch, grass_material},
};
use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings},
    constants::{
        GRASS_WIND_STRENGTH, TERRAIN_GRASS_CHUNK_SIZE, TERRAIN_GRASS_MID_CHUNKS_PER_FRAME, TERRAIN_GRASS_MID_RANGE,
        TERRAIN_GRASS_NEAR_CHUNKS_PER_FRAME, TERRAIN_GRASS_NEAR_RANGE, TERRAIN_GRASS_STREAM_HYSTERESIS,
        TERRAIN_GRASS_STREAM_MARGIN,
    },
    map::{DebugColors, MapLevel},
    materials::{GrassMaterial, TerrainMaterial, terrain_material},
};

#[derive(Component)]
pub struct GrassChunkMarker;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::map) enum ChunkKind {
    Terrain,
    Grounds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::map) struct ChunkKey {
    pub(in crate::map) kind: ChunkKind,
    pub(in crate::map) carrier: CarrierId,
    pub(in crate::map) level: u8,
    pub(in crate::map) x: i32,
    pub(in crate::map) z: i32,
}

// What a chunk's blades scatter over: interior terrain patches clipped to the
// compiled floor footprint, or the exterior height field.
#[derive(Clone)]
pub(crate) enum GrassChunkSource {
    Patches { footprint: Arc<[Floor]> },
    Grounds { grounds: Grounds, cell: IVec2 },
}

// Everything a chunk's mesh is rebuilt from, so a burn can regenerate it.
#[derive(Component, Clone)]
pub struct GrassChunkVisual {
    pub(super) patches: Vec<GrassPatch>,
    pub(super) source: GrassChunkSource,
    pub(super) lod: GrassLod,
    pub(super) origin: Vec3,
    pub(super) green: Color,
}

pub(in crate::map) struct ChunkEntry {
    pub(in crate::map) patches: Vec<GrassPatch>,
    pub(in crate::map) source: GrassChunkSource,
    pub(in crate::map) origin: Vec3,
    pub(in crate::map) carrier: CarrierId,
    pub(in crate::map) parent: Option<Entity>,
    pub(in crate::map) level: Option<MapLevel>,
}

// Every grass chunk the map could show, and the ones built right now. Chunks
// are built only around the camera: building the whole map at once costs
// gigabytes of vertices for blades that are never within their fade range.
#[derive(Resource, Default)]
pub struct GrassChunks {
    entries: BTreeMap<ChunkKey, ChunkEntry>,
    spawned: HashMap<(ChunkKey, GrassLod), Entity>,
    grass_material: Option<Handle<GrassMaterial>>,
    terrain_material: Option<Handle<TerrainMaterial>>,
    green: Color,
    enabled: bool,
}

impl GrassChunks {
    pub(in crate::map) fn register(&mut self, key: ChunkKey, entry: ChunkEntry) {
        self.entries.insert(key, entry);
    }

    pub(crate) fn terrain_material(&self) -> Handle<TerrainMaterial> {
        self.terrain_material
            .clone()
            .expect("terrain material missing before the map spawned")
    }
}

impl GrassLod {
    // A chunk is built once its centre comes this close, and released once
    // it is this far away again, so a player at the edge does not churn it.
    fn stream_radii(self) -> (f32, f32) {
        let fade_end = match self {
            Self::Near => TERRAIN_GRASS_NEAR_RANGE[3],
            Self::Mid => TERRAIN_GRASS_MID_RANGE[3],
        };
        let stream_in = fade_end + TERRAIN_GRASS_STREAM_MARGIN;
        (stream_in, stream_in + TERRAIN_GRASS_STREAM_HYSTERESIS)
    }

    fn chunks_per_frame(self) -> usize {
        match self {
            Self::Near => TERRAIN_GRASS_NEAR_CHUNKS_PER_FRAME,
            Self::Mid => TERRAIN_GRASS_MID_CHUNKS_PER_FRAME,
        }
    }

    fn visibility_range(self) -> VisibilityRange {
        let range = match self {
            Self::Near => TERRAIN_GRASS_NEAR_RANGE,
            Self::Mid => TERRAIN_GRASS_MID_RANGE,
        };
        VisibilityRange {
            start_margin: range[0]..range[1],
            end_margin: range[2]..range[3],
            use_aabb: false,
        }
    }
}

// Runs before the terrain and grounds spawners: they register this map's
// chunks into a cleared registry and take the one terrain material from it.
pub fn grass_chunks_reset_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    debug_colors: Res<DebugColors>,
    settings: Res<ClientSettings>,
    asset_set: Res<AssetSet>,
    server: Res<AssetServer>,
    mut chunks: ResMut<GrassChunks>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
) {
    if !layout.is_changed() && !debug_colors.is_changed() {
        return;
    }
    for entity in chunks.spawned.values() {
        commands.entity(*entity).despawn();
    }
    chunks.spawned.clear();
    chunks.entries.clear();
    chunks.enabled = settings.grass.enabled;
    chunks.green = settings.grass.base_color();
    chunks.grass_material = Some(grass_materials.add(grass_material()));
    chunks.terrain_material = Some(terrain_materials.add(terrain_material(
        &server,
        asset_set.material_by_id(TERRAIN_MATERIAL),
        settings.rendering.texture_anisotropy,
        settings.rendering.mipmaps,
        settings.grass.base_color(),
    )));
}

pub fn grass_streaming_system(
    mut commands: Commands,
    mut chunks: ResMut<GrassChunks>,
    carriers: Res<Carriers>,
    camera: Query<&GlobalTransform, With<MainCameraMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(material) = chunks.grass_material.clone() else {
        return;
    };
    if !chunks.enabled {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();

    let mut released = Vec::new();
    let mut wanted: Vec<(f32, ChunkKey, GrassLod)> = Vec::new();
    for (key, entry) in &chunks.entries {
        let center = carriers.pose(entry.carrier).transform_point(entry.origin);
        let distance = Vec2::new(center.x - eye.x, center.z - eye.z).length();
        for lod in [GrassLod::Near, GrassLod::Mid] {
            let (stream_in, stream_out) = lod.stream_radii();
            match chunks.spawned.get(&(*key, lod)) {
                Some(&entity) if distance > stream_out => {
                    commands.entity(entity).despawn();
                    released.push((*key, lod));
                }
                None if distance <= stream_in => wanted.push((distance, *key, lod)),
                _ => {}
            }
        }
    }
    for slot in released {
        chunks.spawned.remove(&slot);
    }

    wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut budget = [GrassLod::Near.chunks_per_frame(), GrassLod::Mid.chunks_per_frame()];
    for (_, key, lod) in wanted {
        let slot = match lod {
            GrassLod::Near => 0,
            GrassLod::Mid => 1,
        };
        if budget[slot] == 0 {
            continue;
        }
        budget[slot] -= 1;
        let entry = &chunks.entries[&key];
        let visual = GrassChunkVisual {
            patches: entry.patches.clone(),
            source: entry.source.clone(),
            lod,
            origin: entry.origin,
            green: chunks.green,
        };
        // Burns in effect reach a new chunk through `grass_burn_system`,
        // which rebuilds every chunk it sees added.
        let mut chunk = commands.spawn((GrassChunkMarker, Transform::from_translation(entry.origin)));
        if let Some(parent) = entry.parent {
            chunk.insert(ChildOf(parent));
        }
        if let Some(level) = entry.level {
            chunk.insert(level);
        }
        if let Some(mesh) = grass_chunk_mesh(&visual, &[]) {
            let bounds = padded_grass_bounds(&mesh);
            chunk.insert((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Visibility::Visible,
                NotShadowCaster,
                bounds,
                lod.visibility_range(),
            ));
        }
        chunk.insert(visual);
        let entity = chunk.id();
        chunks.spawned.insert((key, lod), entity);
    }
}

pub(super) fn grass_chunk_mesh(visual: &GrassChunkVisual, burns: &[GrassBurn]) -> Option<Mesh> {
    let mesh = match &visual.source {
        GrassChunkSource::Patches { footprint } => {
            let mut merged: Option<Mesh> = None;
            for &patch in &visual.patches {
                let mesh = grass_patch_mesh(patch, footprint, visual.lod, visual.green, burns);
                if mesh.count_vertices() == 0 {
                    continue;
                }
                match &mut merged {
                    Some(chunk) => chunk
                        .merge(&mesh)
                        .expect("terrain grass patches have incompatible vertex layouts"),
                    None => merged = Some(mesh),
                }
            }
            merged?
        }
        GrassChunkSource::Grounds { grounds, cell } => {
            let mesh = grounds_chunk_mesh(grounds, *cell, visual.lod, visual.green, burns);
            (mesh.count_vertices() > 0).then_some(mesh)?
        }
    };
    Some(mesh.transformed_by(Transform::from_translation(-visual.origin)))
}

fn grounds_chunk_mesh(grounds: &Grounds, cell: IVec2, lod: GrassLod, green: Color, burns: &[GrassBurn]) -> Mesh {
    let seed = (cell.x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (cell.y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    grass_scatter_mesh(
        seed,
        lod.tuft_count(TERRAIN_GRASS_CHUNK_SIZE.powi(2)),
        lod,
        green,
        burns,
        |rng| {
            let x = (cell.x as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
            let z = (cell.y as f32 + rng.random::<f32>()) * TERRAIN_GRASS_CHUNK_SIZE;
            (grounds.distance_outside_map(x, z) >= 0.2).then(|| Vec3::new(x, grounds.height(x, z), z))
        },
        |_, _| true,
    )
}

// The mesh bounds plus the farthest the wind can carry a tip, so a chunk at
// the edge of the view is not culled while its blades still sway into it.
pub(super) fn padded_grass_bounds(mesh: &Mesh) -> Aabb {
    let mut bounds = mesh.compute_aabb().expect("grass mesh positions missing");
    let sway = GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    bounds.half_extents.x += sway;
    bounds.half_extents.z += sway;
    bounds
}
