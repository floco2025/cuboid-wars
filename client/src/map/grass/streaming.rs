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
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use common::{
    map::{Carriers, Grounds},
    protocol::{CarrierId, Floor, MapLayout, TERRAIN_MATERIAL},
};
use rand::RngExt;

use super::{
    burn::GrassBurn,
    material::grass_material,
    mesh::{AABB_BASE_PAD, GrassLod, WIND_SWAY_FACTOR, grass_patch_mesh, grass_scatter_mesh},
    patch::GrassPatch,
};
use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings},
    constants::{
        GRASS_CHUNK_SIZE, GRASS_MID_CHUNKS_PER_FRAME, GRASS_MID_RANGE, GRASS_NEAR_CHUNKS_PER_FRAME, GRASS_NEAR_RANGE,
        GRASS_ROCK_CLEARANCE, GRASS_STREAM_HYSTERESIS, GRASS_STREAM_MARGIN, GRASS_WIND_STRENGTH,
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

// A chunk whose mesh is still being built on the compute pool: a near chunk
// is tens of thousands of blades, milliseconds the frame cannot spare while
// a sprint streams several in. The visual joins the entity with the mesh,
// so a burn sees the chunk added once it has blades to burn.
#[derive(Component)]
pub struct GrassChunkBuild {
    visual: GrassChunkVisual,
    task: Task<Option<Mesh>>,
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

struct GroundsGrass {
    grounds: Grounds,
    level: u8,
}

// The interior grass chunks the map could show, the exterior ground whose
// cells are taken from the camera's surroundings, and the chunks built right
// now. Chunks are built only around the camera: building the whole map at
// once costs gigabytes of vertices for blades that are never within their
// fade range.
#[derive(Resource, Default)]
pub struct GrassChunks {
    entries: BTreeMap<ChunkKey, ChunkEntry>,
    grounds: Option<GroundsGrass>,
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

    pub(in crate::map) fn set_grounds(&mut self, grounds: Grounds, level: u8) {
        self.grounds = Some(GroundsGrass { grounds, level });
    }

    pub(crate) fn terrain_material(&self) -> Handle<TerrainMaterial> {
        self.terrain_material
            .clone()
            .expect("terrain material missing before the map spawned")
    }

    pub(crate) fn terrain_material_handle(&self) -> Option<Handle<TerrainMaterial>> {
        self.terrain_material.clone()
    }

    pub(crate) fn grass_material_handle(&self) -> Option<Handle<GrassMaterial>> {
        self.grass_material.clone()
    }

    // Where a chunk's centre is this frame.
    fn center(&self, key: &ChunkKey, carriers: &Carriers) -> Option<Vec3> {
        match key.kind {
            ChunkKind::Terrain => {
                let entry = self.entries.get(key)?;
                Some(carriers.pose(entry.carrier).transform_point(entry.origin))
            }
            ChunkKind::Grounds => {
                let grounds = &self.grounds.as_ref()?.grounds;
                let center = grounds_cell_center(IVec2::new(key.x, key.z));
                Some(Vec3::new(center.x, grounds.height(center.x, center.y), center.y))
            }
        }
    }
}

impl GroundsGrass {
    // One chunk per ten-metre cell outside the map, out to the terrain's edge.
    fn cell_is_meadow(&self, cell: IVec2) -> bool {
        let center = grounds_cell_center(cell);
        let grounds = &self.grounds;
        let half = GRASS_CHUNK_SIZE * 0.5;
        let inside = center.x.abs() + half < grounds.half_size[0] && center.y.abs() + half < grounds.half_size[1];
        !inside && grounds.distance_outside_map(center.x, center.y) - half < grounds.extent()
    }

    fn key(&self, cell: IVec2) -> ChunkKey {
        ChunkKey {
            kind: ChunkKind::Grounds,
            carrier: CarrierId::WORLD,
            level: self.level,
            x: cell.x,
            z: cell.y,
        }
    }

    fn entry(&self, cell: IVec2) -> ChunkEntry {
        let center = grounds_cell_center(cell);
        let origin = Vec3::new(center.x, self.grounds.height(center.x, center.y), center.y);
        let patch = GrassPatch {
            x1: cell.x as f32 * GRASS_CHUNK_SIZE,
            x2: (cell.x + 1) as f32 * GRASS_CHUNK_SIZE,
            z1: cell.y as f32 * GRASS_CHUNK_SIZE,
            z2: (cell.y + 1) as f32 * GRASS_CHUNK_SIZE,
            y: origin.y,
            level: self.level,
            carrier: CarrierId::WORLD,
        };
        ChunkEntry {
            patches: vec![patch],
            source: GrassChunkSource::Grounds {
                grounds: self.grounds.clone(),
                cell,
            },
            origin,
            carrier: CarrierId::WORLD,
            parent: None,
            level: None,
        }
    }
}

fn grounds_cell_center(cell: IVec2) -> Vec2 {
    (cell.as_vec2() + Vec2::splat(0.5)) * GRASS_CHUNK_SIZE
}

impl GrassLod {
    // A chunk is built once its centre comes this close, and released once
    // it is this far away again, so a player at the edge does not churn it.
    fn stream_radii(self) -> (f32, f32) {
        let fade_end = match self {
            Self::Near => GRASS_NEAR_RANGE[3],
            Self::Mid => GRASS_MID_RANGE[3],
        };
        let stream_in = fade_end + GRASS_STREAM_MARGIN;
        (stream_in, stream_in + GRASS_STREAM_HYSTERESIS)
    }

    fn chunks_per_frame(self) -> usize {
        match self {
            Self::Near => GRASS_NEAR_CHUNKS_PER_FRAME,
            Self::Mid => GRASS_MID_CHUNKS_PER_FRAME,
        }
    }

    fn visibility_range(self) -> VisibilityRange {
        let range = match self {
            Self::Near => GRASS_NEAR_RANGE,
            Self::Mid => GRASS_MID_RANGE,
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
    chunks.grounds = None;
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
) {
    if chunks.grass_material.is_none() || !chunks.enabled {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();
    let distance_to = |center: Vec3| Vec2::new(center.x - eye.x, center.z - eye.z).length();

    let mut released = Vec::new();
    for (&(key, lod), &entity) in &chunks.spawned {
        let (_, stream_out) = lod.stream_radii();
        let gone = chunks
            .center(&key, &carriers)
            .is_none_or(|center| distance_to(center) > stream_out);
        if gone {
            commands.entity(entity).despawn();
            released.push((key, lod));
        }
    }
    for slot in released {
        chunks.spawned.remove(&slot);
    }

    let mut wanted: Vec<(f32, ChunkKey, GrassLod)> = Vec::new();
    for (key, entry) in &chunks.entries {
        let distance = distance_to(carriers.pose(entry.carrier).transform_point(entry.origin));
        for lod in [GrassLod::Near, GrassLod::Mid] {
            if distance <= lod.stream_radii().0 && !chunks.spawned.contains_key(&(*key, lod)) {
                wanted.push((distance, *key, lod));
            }
        }
    }
    if let Some(grounds) = &chunks.grounds {
        for lod in [GrassLod::Near, GrassLod::Mid] {
            let (stream_in, _) = lod.stream_radii();
            let low = ((Vec2::new(eye.x, eye.z) - Vec2::splat(stream_in)) / GRASS_CHUNK_SIZE)
                .floor()
                .as_ivec2();
            let high = ((Vec2::new(eye.x, eye.z) + Vec2::splat(stream_in)) / GRASS_CHUNK_SIZE)
                .floor()
                .as_ivec2();
            for z in low.y..=high.y {
                for x in low.x..=high.x {
                    let cell = IVec2::new(x, z);
                    let key = grounds.key(cell);
                    if chunks.spawned.contains_key(&(key, lod)) || !grounds.cell_is_meadow(cell) {
                        continue;
                    }
                    let center = grounds_cell_center(cell);
                    let distance = distance_to(Vec3::new(center.x, eye.y, center.y));
                    if distance <= stream_in {
                        wanted.push((distance, key, lod));
                    }
                }
            }
        }
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
        let grounds_entry;
        let entry = match key.kind {
            ChunkKind::Terrain => &chunks.entries[&key],
            ChunkKind::Grounds => {
                grounds_entry = chunks
                    .grounds
                    .as_ref()
                    .expect("grounds grass cell wanted without grounds")
                    .entry(IVec2::new(key.x, key.z));
                &grounds_entry
            }
        };
        let visual = GrassChunkVisual {
            patches: entry.patches.clone(),
            source: entry.source.clone(),
            lod,
            origin: entry.origin,
            green: chunks.green,
        };
        // Burns in effect reach a new chunk through `grass_burn_system`,
        // which rebuilds every chunk it sees added.
        let mut chunk = commands.spawn((
            GrassChunkMarker,
            Transform::from_translation(entry.origin),
            Visibility::Visible,
        ));
        if let Some(parent) = entry.parent {
            chunk.insert(ChildOf(parent));
        }
        if let Some(level) = entry.level {
            chunk.insert(level);
        }
        let build = visual.clone();
        chunk.insert(GrassChunkBuild {
            visual,
            task: AsyncComputeTaskPool::get().spawn(async move { grass_chunk_mesh(&build, &[]) }),
        });
        let entity = chunk.id();
        chunks.spawned.insert((key, lod), entity);
    }
}

// Gives every chunk whose build has finished its mesh.
pub fn grass_chunk_finish_system(
    mut commands: Commands,
    chunks: Res<GrassChunks>,
    mut builds: Query<(Entity, &mut GrassChunkBuild)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(material) = chunks.grass_material.clone() else {
        return;
    };
    for (entity, mut build) in &mut builds {
        let Some(mesh) = block_on(poll_once(&mut build.task)) else {
            continue;
        };
        let mut chunk = commands.entity(entity);
        chunk.remove::<GrassChunkBuild>();
        if let Some(mesh) = mesh {
            let bounds = padded_grass_bounds(&mesh);
            chunk.insert((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                NotShadowCaster,
                bounds,
                build.visual.lod.visibility_range(),
            ));
        }
        chunk.insert(build.visual.clone());
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

// Blades over the exterior ground, leaving the rocks' footprints bare.
fn grounds_chunk_mesh(grounds: &Grounds, cell: IVec2, lod: GrassLod, green: Color, burns: &[GrassBurn]) -> Mesh {
    let seed = (cell.x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (cell.y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    let min = cell.as_vec2() * GRASS_CHUNK_SIZE;
    let rocks: Vec<(Vec2, f32)> = grounds
        .rocks_within(min, min + Vec2::splat(GRASS_CHUNK_SIZE))
        .into_iter()
        .map(|rock| {
            (
                Vec2::new(rock.position.x, rock.position.z),
                rock.scale.x * GRASS_ROCK_CLEARANCE,
            )
        })
        .collect();
    grass_scatter_mesh(
        seed,
        lod.tuft_count(GRASS_CHUNK_SIZE.powi(2)),
        lod,
        green,
        burns,
        |rng| {
            let x = (cell.x as f32 + rng.random::<f32>()) * GRASS_CHUNK_SIZE;
            let z = (cell.y as f32 + rng.random::<f32>()) * GRASS_CHUNK_SIZE;
            let clear = grounds.distance_outside_map(x, z) >= 0.2
                && rocks
                    .iter()
                    .all(|(center, radius)| center.distance_squared(Vec2::new(x, z)) > radius * radius);
            clear.then(|| Vec3::new(x, grounds.height(x, z), z))
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
