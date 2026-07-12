use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use common::{face_materials::FaceMaterials, map_geometry::MapGeometry};
use serde::Deserialize;

use super::{MaterialRules, query::SegmentMaterials};

const SUPPORTED_MAP_VERSION: u32 = 1;

#[derive(Deserialize)]
struct MapFile {
    version: u32,
    map: MapBody,
}

#[derive(Deserialize)]
struct MapBody {
    grid_cols: i32,
    grid_rows: i32,
    #[serde(default)]
    levels: Vec<MapLevel>,
    #[serde(default)]
    ramps: Vec<MapRamp>,
}

#[derive(Deserialize)]
struct MapLevel {
    #[serde(default)]
    floors: Vec<MapFloor>,
    #[serde(default)]
    inaccessible_floors: Vec<MapFloor>,
    #[serde(default)]
    walls: Vec<MapWall>,
}

#[derive(Deserialize)]
struct MapFloor {
    col: i32,
    row: i32,
    #[serde(flatten)]
    materials: FaceMaterials,
}

#[derive(Deserialize)]
struct MapWall {
    c0: i32,
    r0: i32,
    c1: i32,
    r1: i32,
    #[serde(flatten)]
    materials: FaceMaterials,
}

#[derive(Deserialize)]
struct MapRamp {
    low: [i32; 2],
    high: [i32; 2],
    lower_level: u32,
    #[serde(flatten)]
    materials: FaceMaterials,
}

impl MaterialRules {
    pub(crate) fn load_from_map_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let file: MapFile =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        anyhow::ensure!(
            file.version == SUPPORTED_MAP_VERSION,
            "unsupported map file version {} (expected {})",
            file.version,
            SUPPORTED_MAP_VERSION
        );

        let geometry = MapGeometry::new(file.map.grid_cols, file.map.grid_rows);
        let mut floor_materials: HashMap<(u8, i32, i32), FaceMaterials> = HashMap::new();
        let mut wall_materials: HashMap<(u8, [i32; 2], [i32; 2]), FaceMaterials> = HashMap::new();
        for (level_idx, level) in file.map.levels.iter().enumerate() {
            let level_u8 = u8::try_from(level_idx).expect("more than 256 levels not supported");
            for floor in level.floors.iter().chain(level.inaccessible_floors.iter()) {
                floor_materials.insert((level_u8, floor.col, floor.row), floor.materials.clone());
            }
            for wall in &level.walls {
                let key = wall_edge_key([wall.c0, wall.r0], [wall.c1, wall.r1]);
                wall_materials.insert((level_u8, key.0, key.1), wall.materials.clone());
            }
        }
        let mut ramp_materials: HashMap<(u8, i32, i32), FaceMaterials> = HashMap::new();
        for ramp in &file.map.ramps {
            let lower_level = u8::try_from(ramp.lower_level).expect("ramp lower_level out of u8 range");
            let col_min = ramp.low[0].min(ramp.high[0]);
            let col_max = ramp.low[0].max(ramp.high[0]);
            let row_min = ramp.low[1].min(ramp.high[1]);
            let row_max = ramp.low[1].max(ramp.high[1]);
            for col in col_min..col_max {
                for row in row_min..row_max {
                    ramp_materials.insert((lower_level, col, row), ramp.materials.clone());
                }
            }
        }

        Ok(Self {
            geometry,
            segments: SegmentMaterials {
                floors: floor_materials,
                walls: wall_materials,
                ramps: ramp_materials,
            },
        })
    }
}

// Wall edges are stored normalized so lookup is order-independent.
pub(super) fn wall_edge_key(from: [i32; 2], to: [i32; 2]) -> ([i32; 2], [i32; 2]) {
    if (from[0], from[1]) <= (to[0], to[1]) {
        (from, to)
    } else {
        (to, from)
    }
}

#[cfg(test)]
mod tests {
    use super::super::MaterialRules;
    use crate::{config::ServerGameplayConfig, map::generation::map_path};

    #[test]
    fn default_map_material_rules_load() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        MaterialRules::load_from_map_path(&map_path(&config.default_map))
            .expect("default map's per-segment materials should load");
    }
}
