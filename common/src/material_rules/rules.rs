use std::collections::BTreeMap;

use serde::Deserialize;

use super::{
    FaceMaterials,
    grid::{same_edge, touches_horizontal_line, touches_rectangle, touches_vertical_line},
};
use crate::constants::{GRID_COLS, GRID_ROWS};

#[derive(Debug, Clone)]
pub struct MaterialRuleSet {
    pub(super) floors: Vec<MaterialRule>,
    pub(super) walls: Vec<MaterialRule>,
    pub(super) items: Vec<MaterialRule>,
}

impl<'de> Deserialize<'de> for MaterialRuleSet {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MaterialRulesDef {
            Flat(Vec<FlatMaterialRule>),
            Legacy(LegacyMaterialRules),
        }

        match MaterialRulesDef::deserialize(deserializer)? {
            MaterialRulesDef::Legacy(rules) => Ok(Self {
                floors: rules.floors,
                walls: rules.walls,
                items: rules.items,
            }),
            MaterialRulesDef::Flat(rules) => {
                let mut floors = Vec::new();
                let mut walls = Vec::new();
                let mut items = Vec::new();
                for rule in rules {
                    if let Some(materials) = rule.floors.clone() {
                        floors.push(rule.material_rule(materials, WallRuleRelation::On));
                    }
                    if let Some(materials) = rule.walls.clone() {
                        walls.push(rule.material_rule(materials, WallRuleRelation::On));
                    }
                    if let Some(materials) = rule.touching_walls.clone() {
                        walls.push(rule.material_rule(materials, WallRuleRelation::Touching));
                    }
                    if let Some(item_materials) = rule.items.clone() {
                        for (item_type, material) in item_materials {
                            items.push(rule.item_rule(item_type, material));
                        }
                    }
                }
                Ok(Self { floors, walls, items })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyMaterialRules {
    #[serde(default)]
    floors: Vec<MaterialRule>,
    #[serde(default)]
    walls: Vec<MaterialRule>,
    #[serde(default)]
    items: Vec<MaterialRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct FlatMaterialRule {
    #[serde(default)]
    floors: Option<FaceMaterialRule>,
    #[serde(default)]
    walls: Option<FaceMaterialRule>,
    #[serde(default)]
    touching_walls: Option<FaceMaterialRule>,
    #[serde(default)]
    items: Option<BTreeMap<String, String>>,
    #[serde(default)]
    level: Option<u8>,
    #[serde(default)]
    levels: Option<Vec<u8>>,
    #[serde(default)]
    cols: Option<[i32; 2]>,
    #[serde(default)]
    rows: Option<[i32; 2]>,
    #[serde(default)]
    from: Option<[i32; 2]>,
    #[serde(default)]
    to: Option<[i32; 2]>,
}

impl FlatMaterialRule {
    fn material_rule(&self, materials: FaceMaterialRule, wall_relation: WallRuleRelation) -> MaterialRule {
        MaterialRule {
            material: materials.all.clone(),
            materials: Some(materials),
            level: self.level,
            levels: self.levels.clone(),
            cols: self.cols,
            rows: self.rows,
            from: self.from,
            to: self.to,
            item_type: None,
            wall_relation,
        }
    }

    fn item_rule(&self, item_type: String, material: String) -> MaterialRule {
        MaterialRule {
            material: Some(material),
            materials: None,
            level: self.level,
            levels: self.levels.clone(),
            cols: self.cols,
            rows: self.rows,
            from: self.from,
            to: self.to,
            item_type: (item_type != "all").then_some(item_type),
            wall_relation: WallRuleRelation::On,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct MaterialRule {
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    materials: Option<FaceMaterialRule>,
    #[serde(default)]
    level: Option<u8>,
    #[serde(default)]
    levels: Option<Vec<u8>>,
    #[serde(default)]
    cols: Option<[i32; 2]>,
    #[serde(default)]
    rows: Option<[i32; 2]>,
    #[serde(default)]
    from: Option<[i32; 2]>,
    #[serde(default)]
    to: Option<[i32; 2]>,
    #[serde(default, rename = "type")]
    pub(super) item_type: Option<String>,
    #[serde(skip)]
    wall_relation: WallRuleRelation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum WallRuleRelation {
    #[default]
    On,
    Touching,
}

#[derive(Debug, Clone, Deserialize)]
struct FaceMaterialRule {
    #[serde(default)]
    all: Option<String>,
    #[serde(default)]
    north: Option<String>,
    #[serde(default)]
    south: Option<String>,
    #[serde(default)]
    east: Option<String>,
    #[serde(default)]
    west: Option<String>,
    #[serde(default, alias = "up")]
    top: Option<String>,
    #[serde(default, alias = "down")]
    bottom: Option<String>,
}

impl MaterialRule {
    pub(super) fn matches_level(&self, level: u8) -> bool {
        self.level.is_none_or(|rule_level| rule_level == level)
            && self.levels.as_ref().is_none_or(|levels| levels.contains(&level))
    }

    pub(super) fn matches_cell(&self, col: i32, row: i32) -> bool {
        self.cols.is_none_or(|[min, max]| (min..=max).contains(&col))
            && self.rows.is_none_or(|[min, max]| (min..=max).contains(&row))
    }

    pub(super) fn matches_edge(&self, from: [i32; 2], to: [i32; 2]) -> bool {
        if self.wall_relation == WallRuleRelation::Touching {
            return self.matches_touching_edge(from, to);
        }

        if !self.matches_edge_range(from, to) {
            return false;
        }

        match (self.from, self.to) {
            (Some(rule_from), Some(rule_to)) => same_edge(rule_from, rule_to, from, to),
            (None, None) => true,
            _ => false,
        }
    }

    pub(super) fn specificity(&self) -> u16 {
        let mut score = 0;
        if self.level.is_some() {
            score += 1_000;
        } else if let Some(levels) = &self.levels {
            score += 900_u16.saturating_sub(u16::try_from(levels.len()).unwrap_or(u16::MAX));
        }
        if let Some(range) = self.cols {
            score += range_specificity(range, GRID_COLS);
        }
        if let Some(range) = self.rows {
            score += range_specificity(range, GRID_ROWS);
        }
        if self.from.is_some() && self.to.is_some() {
            score += 1_000;
        }
        if self.item_type.is_some() {
            score += 1_000;
        }
        if self.wall_relation == WallRuleRelation::Touching {
            score = score.saturating_sub(1);
        }
        score
    }

    fn matches_edge_range(&self, from: [i32; 2], to: [i32; 2]) -> bool {
        let min_col = from[0].min(to[0]);
        let max_col = from[0].max(to[0]);
        let min_row = from[1].min(to[1]);
        let max_row = from[1].max(to[1]);

        self.cols
            .is_none_or(|[rule_min, rule_max]| rule_min <= min_col && max_col <= rule_max)
            && self
                .rows
                .is_none_or(|[rule_min, rule_max]| rule_min <= min_row && max_row <= rule_max)
    }

    fn matches_touching_edge(&self, from: [i32; 2], to: [i32; 2]) -> bool {
        match (self.cols, self.rows) {
            (Some([col_min, col_max]), Some([row_min, row_max])) if col_min == col_max => {
                touches_vertical_line(from, to, col_min, [row_min, row_max])
            }
            (Some([col_min, col_max]), Some([row_min, row_max])) if row_min == row_max => {
                touches_horizontal_line(from, to, row_min, [col_min, col_max])
            }
            (Some(cols), Some(rows)) => touches_rectangle(from, to, cols, rows),
            (Some([col_min, col_max]), None) if col_min == col_max => {
                touches_vertical_line(from, to, col_min, [i32::MIN, i32::MAX])
            }
            (None, Some([row_min, row_max])) if row_min == row_max => {
                touches_horizontal_line(from, to, row_min, [i32::MIN, i32::MAX])
            }
            _ => false,
        }
    }

    pub(super) fn has_edge_scope(&self) -> bool {
        self.cols.is_some() || self.rows.is_some() || self.from.is_some() || self.to.is_some()
    }

    pub(super) fn face_materials(&self) -> FaceMaterials {
        let surfaces = self.materials.as_ref();
        let fallback = self
            .material
            .as_deref()
            .or_else(|| surfaces.and_then(|materials| materials.all.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.north.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.south.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.east.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.west.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.top.as_deref()))
            .or_else(|| surfaces.and_then(|materials| materials.bottom.as_deref()))
            .expect("asset material rule must define `material` or at least one material surface");

        FaceMaterials {
            north: surfaces
                .and_then(|materials| materials.north.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
            south: surfaces
                .and_then(|materials| materials.south.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
            east: surfaces
                .and_then(|materials| materials.east.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
            west: surfaces
                .and_then(|materials| materials.west.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
            top: surfaces
                .and_then(|materials| materials.top.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
            bottom: surfaces
                .and_then(|materials| materials.bottom.as_deref())
                .unwrap_or(fallback)
                .to_owned(),
        }
    }

    pub(super) fn item_material(&self) -> &str {
        self.material
            .as_deref()
            .expect("asset item material rule must define `material`")
    }
}

fn range_specificity([min, max]: [i32; 2], full_len: i32) -> u16 {
    let len = (max - min + 1).clamp(1, full_len);
    100 + u16::try_from(full_len - len).expect("grid range length should fit in u16")
}
