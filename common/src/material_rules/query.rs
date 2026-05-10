use std::collections::HashMap;

use crate::{
    map_geometry::MapGeometry,
    protocol::{Floor, ItemType, Ramp, Wall},
};

use super::{
    FaceMaterials,
    grid::{floor_cells, ramp_cells, ramp_lower_level, wall_edges},
    loading::wall_edge_key,
};

#[derive(Debug, Clone, Default)]
pub(super) struct SegmentMaterials {
    // Keyed by (level, col, row).
    pub(super) floors: HashMap<(u8, i32, i32), FaceMaterials>,
    // Keyed by (level, normalized edge endpoints).
    pub(super) walls: HashMap<(u8, [i32; 2], [i32; 2]), FaceMaterials>,
    // Keyed by (lower_level, col, row) for each cell in the ramp footprint.
    pub(super) ramps: HashMap<(u8, i32, i32), FaceMaterials>,
}

#[derive(Debug, Clone)]
pub struct MaterialRules {
    pub(super) geometry: MapGeometry,
    pub(super) segments: SegmentMaterials,
    pub(super) item_materials: HashMap<String, String>,
}

impl MaterialRules {
    #[must_use]
    pub fn geometry(&self) -> &MapGeometry {
        &self.geometry
    }

    #[must_use]
    pub fn materials_for_floor(&self, floor: &Floor) -> FaceMaterials {
        for (col, row) in floor_cells(&self.geometry, floor) {
            if let Some(materials) = self.segments.floors.get(&(floor.level, col, row)) {
                return materials.clone();
            }
        }
        // Fallback: use the segment at the world midpoint.
        let mid_col = self.geometry.world_x_to_cell_col(f32::midpoint(floor.x1, floor.x2));
        let mid_row = self.geometry.world_z_to_cell_row(f32::midpoint(floor.z1, floor.z2));
        if let Some(materials) = self.segments.floors.get(&(floor.level, mid_col, mid_row)) {
            return materials.clone();
        }
        // Stacked wall trim strips sit on wall edges and don't correspond to a
        // floor cell. Use the adjacent wall's materials so the trim visually
        // matches the wall it's filling.
        if let Some(materials) = self.adjacent_wall_materials(floor.level, mid_col, mid_row) {
            return materials;
        }
        self.first_floor_material_on_level(floor.level)
            .unwrap_or_else(missing_materials)
    }

    fn adjacent_wall_materials(&self, level: u8, col: i32, row: i32) -> Option<FaceMaterials> {
        let candidates = [
            wall_edge_key([col, row], [col + 1, row]),
            wall_edge_key([col, row + 1], [col + 1, row + 1]),
            wall_edge_key([col, row], [col, row + 1]),
            wall_edge_key([col + 1, row], [col + 1, row + 1]),
        ];
        for (a, b) in candidates {
            if let Some(m) = self.segments.walls.get(&(level, a, b)) {
                return Some(m.clone());
            }
        }
        None
    }

    fn first_floor_material_on_level(&self, level: u8) -> Option<FaceMaterials> {
        self.segments
            .floors
            .iter()
            .find(|((l, _, _), _)| *l == level)
            .map(|(_, m)| m.clone())
    }

    #[must_use]
    pub fn materials_for_wall(&self, wall: &Wall) -> FaceMaterials {
        for (from, to) in wall_edges(&self.geometry, wall) {
            let (a, b) = wall_edge_key(from, to);
            if let Some(materials) = self.segments.walls.get(&(wall.level, a, b)) {
                return materials.clone();
            }
        }
        let from = [
            self.geometry.world_x_to_grid_col(wall.x1),
            self.geometry.world_z_to_grid_row(wall.z1),
        ];
        let to = [
            self.geometry.world_x_to_grid_col(wall.x2),
            self.geometry.world_z_to_grid_row(wall.z2),
        ];
        let (a, b) = wall_edge_key(from, to);
        if let Some(materials) = self.segments.walls.get(&(wall.level, a, b)) {
            return materials.clone();
        }
        self.first_wall_material_on_level(wall.level)
            .unwrap_or_else(missing_materials)
    }

    fn first_wall_material_on_level(&self, level: u8) -> Option<FaceMaterials> {
        self.segments
            .walls
            .iter()
            .find(|((l, _, _), _)| *l == level)
            .map(|(_, m)| m.clone())
    }

    #[must_use]
    pub fn materials_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> FaceMaterials {
        let (a, b) = wall_edge_key(from, to);
        if let Some(materials) = self.segments.walls.get(&(level, a, b)) {
            return materials.clone();
        }
        self.first_wall_material_on_level(level)
            .unwrap_or_else(missing_materials)
    }

    #[must_use]
    pub fn materials_for_ramp_top(&self, ramp: &Ramp) -> FaceMaterials {
        let lower_level = ramp_lower_level(ramp);
        for (col, row) in ramp_cells(&self.geometry, ramp) {
            if let Some(materials) = self.segments.ramps.get(&(lower_level, col, row)) {
                return materials.clone();
            }
            // Fall back to floor materials at the ramp footprint cell.
            if let Some(materials) = self.segments.floors.get(&(lower_level, col, row)) {
                return materials.clone();
            }
        }
        missing_materials()
    }

    #[must_use]
    pub fn materials_for_ramp_side(&self, ramp: &Ramp) -> FaceMaterials {
        // Ramp sides reuse the ramp's own face materials when present, else
        // fall back to floor materials at the ramp's footprint cell.
        self.materials_for_ramp_top(ramp)
    }

    #[must_use]
    pub fn material_for_item(&self, item_type: ItemType) -> &str {
        let name = item_type_name(item_type);
        self.item_materials
            .get(name)
            .or_else(|| self.item_materials.get("default"))
            .map(String::as_str)
            .expect("item_materials must define `default`")
    }
}

fn item_type_name(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::SpeedPowerUp => "SpeedPowerUp",
        ItemType::MultiShotPowerUp => "MultiShotPowerUp",
        ItemType::PhasingPowerUp => "PhasingPowerUp",
        ItemType::Cookie => "Cookie",
    }
}

fn missing_materials() -> FaceMaterials {
    // Returned when a query lands on a segment that wasn't in the map. The
    // mesh emission paths shouldn't hit this in normal operation; the value is
    // visually distinctive so the issue is obvious if it does.
    FaceMaterials::uniform("__missing__")
}
