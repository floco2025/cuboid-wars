use std::collections::HashMap;

use common::{config::MapGeometryConfig, map::MapGeometry, protocol::FaceMaterials};

use super::{MaterialRules, query::SegmentMaterials};
use crate::map::definition::MapDef;

impl MaterialRules {
    pub(crate) fn from_def(map_def: &MapDef, sizes: MapGeometryConfig) -> Self {
        let geometry = MapGeometry::new(map_def.grid_cols, map_def.grid_rows, sizes);
        let mut floor_materials: HashMap<(u8, i32, i32), FaceMaterials> = HashMap::new();
        let mut wall_materials: HashMap<(u8, [i32; 2], [i32; 2]), FaceMaterials> = HashMap::new();
        for (level_idx, level) in map_def.levels.iter().enumerate() {
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
        for ramp in &map_def.ramps {
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

        Self {
            geometry,
            segments: SegmentMaterials {
                floors: floor_materials,
                walls: wall_materials,
                ramps: ramp_materials,
            },
        }
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
#[path = "tests/loading.rs"]
mod tests;
