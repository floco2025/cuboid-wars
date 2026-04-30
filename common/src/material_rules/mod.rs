mod grid;
mod resolve;
mod rules;

use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::protocol::{Floor, ItemType, Ramp, Wall};

use self::{
    grid::{
        floor_cells, ramp_cells, ramp_lower_level, wall_edges, world_x_to_cell_col, world_x_to_grid_col,
        world_z_to_cell_row, world_z_to_grid_row,
    },
    resolve::{resolve_directional_materials, resolve_item_material},
    rules::MaterialRuleSet,
};

pub use grid::{grid_col_to_world_x, grid_row_to_world_z};

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialRules {
    pub material_rules: MaterialRuleSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceMaterials {
    pub north: String,
    pub south: String,
    pub east: String,
    pub west: String,
    pub top: String,
    pub bottom: String,
}

impl FaceMaterials {
    #[must_use]
    pub fn uniform(material: impl Into<String>) -> Self {
        let material = material.into();
        Self {
            north: material.clone(),
            south: material.clone(),
            east: material.clone(),
            west: material.clone(),
            top: material.clone(),
            bottom: material,
        }
    }

    #[must_use]
    pub fn is_uniform(&self) -> bool {
        self.north == self.south
            && self.north == self.east
            && self.north == self.west
            && self.north == self.top
            && self.north == self.bottom
    }

    #[must_use]
    pub fn first(&self) -> &str {
        &self.top
    }
}

impl MaterialRules {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../client/assets/default.json"
        )))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    #[must_use]
    pub fn materials_for_floor(&self, floor: &Floor) -> FaceMaterials {
        let mut material = None;
        for (col, row) in floor_cells(floor) {
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
            let col = world_x_to_cell_col(f32::midpoint(floor.x1, floor.x2));
            let row = world_z_to_cell_row(f32::midpoint(floor.z1, floor.z2));
            self.materials_for_floor_cell(floor.level, col, row)
        })
    }

    #[must_use]
    pub fn materials_for_wall(&self, wall: &Wall) -> FaceMaterials {
        let mut material = None;
        for (from, to) in wall_edges(wall) {
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
                [world_x_to_grid_col(wall.x1), world_z_to_grid_row(wall.z1)],
                [world_x_to_grid_col(wall.x2), world_z_to_grid_row(wall.z2)],
            )
        })
    }

    #[must_use]
    pub fn materials_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> FaceMaterials {
        resolve_directional_materials(&self.material_rules.walls, |rule| {
            rule.matches_level(level) && rule.matches_edge(from, to)
        })
    }

    #[must_use]
    pub fn materials_for_ramp_top(&self, ramp: &Ramp) -> FaceMaterials {
        let lower_level = ramp_lower_level(ramp);
        let mut material = None;
        for (col, row) in ramp_cells(ramp) {
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
            let col = world_x_to_cell_col(f32::midpoint(ramp.x1, ramp.x2));
            let row = world_z_to_cell_row(f32::midpoint(ramp.z1, ramp.z2));
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
        resolve_item_material(&self.material_rules.items, |rule| {
            rule.item_type
                .as_deref()
                .is_none_or(|rule_type| rule_type == item_type_name)
        })
    }

    #[must_use]
    fn materials_for_floor_cell(&self, level: u8, col: i32, row: i32) -> FaceMaterials {
        resolve_directional_materials(&self.material_rules.floors, |rule| {
            rule.matches_level(level) && rule.matches_cell(col, row)
        })
    }

    #[must_use]
    fn materials_for_interior_wall(&self, level: u8) -> FaceMaterials {
        resolve_directional_materials(&self.material_rules.walls, |rule| {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rules(json: &str) -> MaterialRules {
        serde_json::from_str(json).expect("material rules should parse")
    }

    #[test]
    fn specific_wall_rule_wins_over_level_default() {
        let rules = parse_rules(
            r#"
            {
              "material_rules": [
                { "walls": { "all": "fallback" } },
                { "level": 2, "walls": { "all": "level-wall" } },
                { "level": 2, "from": [4, 5], "to": [5, 5], "walls": { "all": "single-edge" } }
              ]
            }
            "#,
        );

        assert_eq!(rules.materials_for_wall_edge(2, [4, 5], [5, 5]).first(), "single-edge");
        assert_eq!(rules.materials_for_wall_edge(2, [5, 5], [6, 5]).first(), "level-wall");
        assert_eq!(rules.materials_for_wall_edge(1, [4, 5], [5, 5]).first(), "fallback");
    }

    #[test]
    fn touching_wall_rule_matches_edges_that_touch_the_scope() {
        let rules = parse_rules(
            r#"
            {
              "material_rules": [
                { "walls": { "all": "fallback" } },
                {
                  "level": 2,
                  "cols": [5, 5],
                  "rows": [5, 8],
                  "touching_walls": {
                    "all": "touch-default",
                    "east": "touch-east"
                  }
                }
              ]
            }
            "#,
        );

        let materials = rules.materials_for_wall_edge(2, [4, 6], [5, 6]);
        assert_eq!(materials.east, "touch-east");
        assert_eq!(materials.west, "touch-default");

        assert_eq!(
            rules.materials_for_wall_edge(2, [5, 6], [6, 6]).first(),
            "touch-default"
        );
        assert_eq!(rules.materials_for_wall_edge(2, [6, 6], [7, 6]).first(), "fallback");
    }

    #[test]
    fn same_specificity_conflicts_panic() {
        let rules = parse_rules(
            r#"
            {
              "material_rules": [
                { "walls": { "all": "fallback" } },
                { "level": 2, "cols": [5, 5], "walls": { "all": "a" } },
                { "level": 2, "cols": [5, 5], "walls": { "all": "b" } }
              ]
            }
            "#,
        );

        let result = std::panic::catch_unwind(|| rules.materials_for_wall_edge(2, [5, 4], [5, 5]));
        assert!(result.is_err());
    }
}
