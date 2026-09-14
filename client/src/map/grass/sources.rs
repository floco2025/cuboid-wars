use super::{patch::GrassPatch, streaming::GrassChunks};
use crate::{
    constants::GRASS_CHUNK_SIZE,
    map::{DebugColors, MapLevel},
};
use bevy::prelude::*;
use common::{
    map::{Carriers, Grounds},
    protocol::{CarrierId, Floor, MapLayout},
};
use std::{collections::BTreeMap, sync::Arc};

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

pub(in crate::map) struct ChunkEntry {
    pub(in crate::map) patches: Vec<GrassPatch>,
    pub(in crate::map) source: GrassChunkSource,
    pub(in crate::map) origin: Vec3,
    pub(in crate::map) carrier: CarrierId,
    pub(in crate::map) parent: Option<Entity>,
    pub(in crate::map) level: Option<MapLevel>,
}

pub(super) struct GroundsGrass {
    grounds: Grounds,
    level: u8,
}

#[derive(Resource, Default)]
pub struct GrassSources {
    pub(super) entries: BTreeMap<ChunkKey, ChunkEntry>,
    pub(super) grounds: Option<GroundsGrass>,
}

impl GrassSources {
    pub(in crate::map) fn register(&mut self, key: ChunkKey, entry: ChunkEntry) {
        self.entries.insert(key, entry);
    }
    // Where a chunk's centre is this frame.
    pub(super) fn center(&self, key: &ChunkKey, carriers: &Carriers) -> Option<Vec3> {
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
    pub(super) fn cell_is_meadow(&self, cell: IVec2) -> bool {
        let center = grounds_cell_center(cell);
        let grounds = &self.grounds;
        let half = GRASS_CHUNK_SIZE * 0.5;
        let inside = grounds.distance_outside_footprint(center.x, center.y) + half < 0.0;
        !inside && grounds.distance_outside_bounds(center.x, center.y) - half < grounds.extent()
    }

    pub(super) fn key(&self, cell: IVec2) -> ChunkKey {
        ChunkKey {
            kind: ChunkKind::Grounds,
            carrier: CarrierId::WORLD,
            level: self.level,
            x: cell.x,
            z: cell.y,
        }
    }

    pub(super) fn entry(&self, cell: IVec2) -> ChunkEntry {
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

pub(super) fn grounds_cell_center(cell: IVec2) -> Vec2 {
    (cell.as_vec2() + Vec2::splat(0.5)) * GRASS_CHUNK_SIZE
}

pub fn grass_sources_reset_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    debug_colors: Res<DebugColors>,
    mut sources: ResMut<GrassSources>,
    mut chunks: ResMut<GrassChunks>,
) {
    if !layout.is_changed() && !debug_colors.is_changed() {
        return;
    }
    chunks.spawned.retain(|(key, _), entity| {
        if layout.is_changed() || key.kind == ChunkKind::Terrain {
            commands.entity(*entity).despawn();
            false
        } else {
            true
        }
    });
    sources.entries.clear();
    if layout.is_changed() {
        sources.grounds = layout.grounds.clone().map(|grounds| GroundsGrass {
            level: grounds.settings.level,
            grounds,
        });
    }
}

#[cfg(test)]
#[path = "tests/sources.rs"]
mod tests;
