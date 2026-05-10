use crate::{
    map_geometry::MapGeometry,
    protocol::{Floor, ItemType, Ramp, Wall},
};

use super::{
    FaceMaterials,
    grid::{floor_cells, ramp_cells, ramp_lower_level, wall_edges},
    resolve::{resolve_face_materials, resolve_item_material},
    rules::RuleSet,
};

#[derive(Debug, Clone)]
pub struct MaterialRules {
    pub(super) geometry: MapGeometry,
    pub(super) rules: RuleSet,
}

impl MaterialRules {
    #[must_use]
    pub fn geometry(&self) -> &MapGeometry {
        &self.geometry
    }

    #[must_use]
    pub fn materials_for_floor(&self, floor: &Floor) -> FaceMaterials {
        let mut material = None;
        for (col, row) in floor_cells(&self.geometry, floor) {
            let next = self.materials_for_floor_cell(floor.level, col, row);
            if let Some(current) = &material {
                assert_eq!(
                    *current, next,
                    "floor segment at level {} spans multiple materials: {current:?} and {next:?}",
                    floor.level
                );
            }
            material = Some(next);
        }

        material.unwrap_or_else(|| {
            let col = self.geometry.world_x_to_cell_col(f32::midpoint(floor.x1, floor.x2));
            let row = self.geometry.world_z_to_cell_row(f32::midpoint(floor.z1, floor.z2));
            self.materials_for_floor_cell(floor.level, col, row)
        })
    }

    #[must_use]
    pub fn materials_for_wall(&self, wall: &Wall) -> FaceMaterials {
        let mut material = None;
        for (from, to) in wall_edges(&self.geometry, wall) {
            let next = self.materials_for_wall_edge(wall.level, from, to);
            if let Some(current) = &material {
                assert_eq!(
                    *current, next,
                    "wall segment at level {} spans multiple materials: {current:?} and {next:?}",
                    wall.level
                );
            }
            material = Some(next);
        }

        material.unwrap_or_else(|| {
            self.materials_for_wall_edge(
                wall.level,
                [
                    self.geometry.world_x_to_grid_col(wall.x1),
                    self.geometry.world_z_to_grid_row(wall.z1),
                ],
                [
                    self.geometry.world_x_to_grid_col(wall.x2),
                    self.geometry.world_z_to_grid_row(wall.z2),
                ],
            )
        })
    }

    #[must_use]
    pub fn materials_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> FaceMaterials {
        resolve_face_materials(&self.rules.walls, self.geometry.grid_cols, self.geometry.grid_rows, |rule| {
            rule.matches_level(level) && rule.matches_edge(from, to)
        })
    }

    #[must_use]
    pub fn materials_for_ramp_top(&self, ramp: &Ramp) -> FaceMaterials {
        let lower_level = ramp_lower_level(ramp);
        let mut material = None;
        for (col, row) in ramp_cells(&self.geometry, ramp) {
            let next = self.materials_for_floor_cell(lower_level, col, row);
            if let Some(current) = &material {
                assert_eq!(
                    *current, next,
                    "ramp top at lower level {lower_level} spans multiple floor materials: {current:?} and {next:?}"
                );
            }
            material = Some(next);
        }

        material.unwrap_or_else(|| {
            let col = self.geometry.world_x_to_cell_col(f32::midpoint(ramp.x1, ramp.x2));
            let row = self.geometry.world_z_to_cell_row(f32::midpoint(ramp.z1, ramp.z2));
            self.materials_for_floor_cell(lower_level, col, row)
        })
    }

    #[must_use]
    pub fn materials_for_ramp_side(&self, ramp: &Ramp) -> FaceMaterials {
        let lower_level = ramp_lower_level(ramp);
        self.materials_for_interior_wall(lower_level)
    }

    #[must_use]
    pub fn material_for_item(&self, item_type: ItemType) -> &str {
        let item_type_name = item_type_name(item_type);
        resolve_item_material(
            &self.rules.items,
            self.geometry.grid_cols,
            self.geometry.grid_rows,
            |rule| {
                rule.item_type
                    .as_deref()
                    .is_none_or(|rule_type| rule_type == item_type_name)
            },
        )
    }

    #[must_use]
    fn materials_for_floor_cell(&self, level: u8, col: i32, row: i32) -> FaceMaterials {
        resolve_face_materials(
            &self.rules.floors,
            self.geometry.grid_cols,
            self.geometry.grid_rows,
            |rule| rule.matches_level(level) && rule.matches_cell(col, row),
        )
    }

    #[must_use]
    fn materials_for_interior_wall(&self, level: u8) -> FaceMaterials {
        resolve_face_materials(&self.rules.walls, self.geometry.grid_cols, self.geometry.grid_rows, |rule| {
            rule.matches_level(level) && !rule.has_edge_scope()
        })
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
