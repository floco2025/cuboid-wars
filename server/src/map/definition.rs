use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{
    floors,
    lights::generate_wall_lights,
    mask::{Mask, mark_has_floor, mark_has_floor_above, mark_has_floor_slab},
    ramps, walls,
};
use crate::{
    constants::FLOOR_OVERLAP,
    resources::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnZone, SpawnEntry},
};
use common::{
    constants::*,
    material_rules::MaterialRules,
    protocol::{Floor, MapLayout, Wall},
};

mod validation;

use validation::{canonicalize, validate_file};

#[cfg(test)]
use validation::validate_map;

const SUPPORTED_VERSION: u32 = 1;

// ============================================================================
// JSON schema
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct MapFile {
    pub version: u32,
    pub map: MapDef,
}

#[derive(Debug, Deserialize)]
pub struct MapDef {
    pub grid_cols: i32,
    pub grid_rows: i32,
    #[serde(default)]
    pub actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    #[serde(default)]
    pub player_spawn_zones: Vec<PlayerSpawnZoneDef>,
    pub levels: Vec<LevelDef>,
    #[serde(default)]
    pub ramps: Vec<RampDef>,
}

#[derive(Debug, Deserialize)]
pub struct LevelDef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub floors: Vec<[i32; 2]>,
    #[serde(default)]
    pub inaccessible_floors: Vec<[i32; 2]>,
    #[serde(default)]
    pub walls: Vec<[i32; 4]>,
}

#[derive(Debug, Deserialize)]
pub struct RampDef {
    pub low: [i32; 2],
    pub high: [i32; 2],
    pub lower_level: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SpawnEntryDef {
    pub kind: String,
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ActorSpawnZoneDef {
    pub level: u32,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub spawns: Vec<SpawnEntryDef>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PlayerSpawnZoneDef {
    pub level: u32,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
}

// ============================================================================
// Loading + validation
// ============================================================================

pub fn load_map(path: &Path) -> Result<MapDef> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let mut file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    validate_file(&file).with_context(|| format!("validating map at {}", path.display()))?;
    canonicalize(&mut file.map);
    Ok(file.map)
}

// ============================================================================
// Compile map source -> MapLayout + MapConfig
// ============================================================================

#[must_use]
pub fn compile_map(map_def: &MapDef, assets: &MaterialRules) -> (MapLayout, MapConfig) {
    let cols = map_def.grid_cols;
    let rows = map_def.grid_rows;

    let ramp_specs: Vec<ramps::RampSpec> = map_def.ramps.iter().map(ramp_spec_from_def).collect();

    let regular_floor_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| {
            let mut m = empty_mask(cols, rows);
            for [col, row] in &level.floors {
                m[*row as usize][*col as usize] = true;
            }
            m
        })
        .collect();
    let slab_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| {
            let mut m = empty_mask(cols, rows);
            for [col, row] in level.floors.iter().chain(level.inaccessible_floors.iter()) {
                m[*row as usize][*col as usize] = true;
            }
            m
        })
        .collect();

    let mut level_grids: Vec<LevelGrid> = map_def
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, level)| {
            let mut cell_grid = CellGrid::new(cols, rows);
            let mut edge_grid = EdgeGrid::new(cols, rows);
            mark_has_floor(&mut cell_grid, &regular_floor_masks[level_idx]);
            mark_has_floor_slab(&mut cell_grid, &slab_masks[level_idx]);
            for wall in &level.walls {
                set_wall_edge(&mut edge_grid, *wall);
            }
            LevelGrid {
                cells: cell_grid,
                edges: edge_grid,
            }
        })
        .collect();

    // Apply ramp flags to each lower-level cell grid. Spawn selection skips
    // these cells on any level.
    for (level_idx, level_grid) in level_grids.iter_mut().enumerate() {
        let level_u32 = u32::try_from(level_idx).unwrap_or(u32::MAX);
        ramps::apply_to_level_cells(&mut level_grid.cells, &ramp_specs, level_u32);
    }
    for level_idx in 0..level_grids.len().saturating_sub(1) {
        mark_has_floor_above(&mut level_grids[level_idx].cells, &slab_masks[level_idx + 1]);
    }

    let mut wall_lights = Vec::new();
    for level_idx in 0..level_grids.len() {
        wall_lights.extend(generate_wall_lights(&level_grids, level_idx));
    }

    let mut all_walls: Vec<Wall> = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut tier = walls::generate_walls(&level_grid.edges, cols, rows, level_u8);
        tier = walls::merge_walls(tier, assets);
        all_walls.extend(tier);
    }

    let mut all_floors: Vec<Floor> = Vec::new();
    for (level_idx, m) in slab_masks.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let y = f32::from(level_u8) * LEVEL_HEIGHT;
        let mut tier = floors::emit_floor_tier(m, cols, rows, level_u8, y);
        if level_idx > 0 {
            tier.extend(floors::emit_stacked_wall_trim(
                &level_grids[level_idx - 1].edges,
                &level_grids[level_idx].edges,
                m,
                cols,
                rows,
                level_u8,
                y,
            ));
        }
        if !FLOOR_OVERLAP {
            tier = floors::merge_floors(tier, assets);
        }
        all_floors.extend(tier);
    }

    let map_layout = MapLayout {
        walls: all_walls,
        ramps: ramps::specs_to_ramps(&ramp_specs),
        wall_lights,
        floors: all_floors,
    };

    (
        map_layout,
        MapConfig {
            levels: level_grids,
            actor_spawn_zones: actor_spawn_zones(map_def),
            player_spawn_zones: player_spawn_zones(map_def),
        },
    )
}

fn ramp_spec_from_def(r: &RampDef) -> ramps::RampSpec {
    ramps::RampSpec {
        lower_level: r.lower_level,
        low: r.low,
        high: r.high,
    }
}

fn empty_mask(grid_cols: i32, grid_rows: i32) -> Mask {
    vec![vec![false; grid_cols as usize]; grid_rows as usize]
}

fn actor_spawn_zones(map_def: &MapDef) -> Vec<ActorSpawnZone> {
    map_def
        .actor_spawn_zones
        .iter()
        .map(|zone| ActorSpawnZone {
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
            spawns: zone
                .spawns
                .iter()
                .map(|entry| SpawnEntry {
                    kind: entry.kind.clone(),
                    count: entry.count,
                })
                .collect(),
        })
        .collect()
}

fn player_spawn_zones(map_def: &MapDef) -> Vec<PlayerSpawnZone> {
    map_def
        .player_spawn_zones
        .iter()
        .map(|zone| PlayerSpawnZone {
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
        })
        .collect()
}

fn set_wall_edge(edges: &mut EdgeGrid, wall: [i32; 4]) {
    let [c0, r0, c1, r1] = wall;
    if r0 == r1 {
        edges.horizontal[r0 as usize][c0.min(c1) as usize] = true;
    } else {
        edges.vertical[r0.min(r1) as usize][c0 as usize] = true;
    }
}

#[cfg(test)]
mod tests;
