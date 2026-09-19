use std::collections::HashMap;

use common::{config::MapGeometryConfig, map::MapGeometry, protocol::FaceMaterials};

use super::{MaterialRules, query::SegmentMaterials};
use map_core::schema::MapDef;

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
            for terrain in &level.terrain {
                floor_materials.insert((level_u8, terrain.col, terrain.row), terrain.materials.clone());
            }
            for wall in &level.walls {
                let key = wall_edge_key([wall.c0, wall.r0], [wall.c1, wall.r1]);
                wall_materials.insert((level_u8, key.0, key.1), wall.materials.clone());
            }
        }
        Self {
            geometry,
            segments: SegmentMaterials {
                floors: floor_materials,
                walls: wall_materials,
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
