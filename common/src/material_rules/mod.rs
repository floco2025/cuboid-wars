mod grid;
mod resolve;
mod rules;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, de};

use crate::protocol::{Floor, ItemType, Ramp, Wall};

use self::{
    grid::{
        floor_cells, ramp_cells, ramp_lower_level, wall_edges, world_x_to_cell_col, world_x_to_grid_col,
        world_z_to_cell_row, world_z_to_grid_row,
    },
    resolve::{resolve_face_materials, resolve_item_material},
    rules::{LayerNames, RuleSet, RuleSetDef},
};

pub use grid::{grid_col_to_world_x, grid_row_to_world_z};

#[derive(Debug, Clone)]
pub struct MaterialRules {
    rules: RuleSet,
}

impl<'de> Deserialize<'de> for MaterialRules {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MaterialRulesDef {
            #[serde(default, alias = "layer_names")]
            layers: LayerNamesDef,
            #[serde(rename = "material_rules")]
            rules: RuleSetDef,
        }

        let def = MaterialRulesDef::deserialize(deserializer)?;
        let rules = RuleSet::from_def(def.rules, &def.layers.0).map_err(de::Error::custom)?;
        Ok(Self { rules })
    }
}

#[derive(Debug, Clone, Default)]
struct LayerNamesDef(LayerNames);

impl<'de> Deserialize<'de> for LayerNamesDef {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Def {
            Ordered(Vec<String>),
            Named(BTreeMap<String, u8>),
        }

        let layers = match Def::deserialize(deserializer)? {
            Def::Ordered(names) => ordered_layer_names(names).map_err(de::Error::custom)?,
            Def::Named(names) => names,
        };
        Ok(Self(layers))
    }
}

fn ordered_layer_names(names: Vec<String>) -> std::result::Result<LayerNames, String> {
    let mut layers = LayerNames::new();
    for (idx, name) in names.into_iter().enumerate() {
        let level =
            u8::try_from(idx).map_err(|_| "material layer list cannot contain more than 256 layers".to_owned())?;
        if layers.insert(name.clone(), level).is_some() {
            return Err(format!("duplicate material layer name {name:?}"));
        }
    }
    Ok(layers)
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
    pub fn primary(&self) -> &str {
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
        resolve_face_materials(&self.rules.walls, |rule| {
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
        resolve_item_material(&self.rules.items, |rule| {
            rule.item_type
                .as_deref()
                .is_none_or(|rule_type| rule_type == item_type_name)
        })
    }

    #[must_use]
    fn materials_for_floor_cell(&self, level: u8, col: i32, row: i32) -> FaceMaterials {
        resolve_face_materials(&self.rules.floors, |rule| {
            rule.matches_level(level) && rule.matches_cell(col, row)
        })
    }

    #[must_use]
    fn materials_for_interior_wall(&self, level: u8) -> FaceMaterials {
        resolve_face_materials(&self.rules.walls, |rule| {
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
