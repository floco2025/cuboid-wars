use std::collections::BTreeMap;

use serde::Deserialize;

use super::{
    FaceMaterials,
    grid::{same_edge, touches_horizontal_line, touches_rectangle, touches_vertical_line},
};
use crate::constants::{GRID_COLS, GRID_ROWS};

#[derive(Debug, Clone)]
pub struct RuleSet {
    pub(super) floors: Vec<MaterialRule>,
    pub(super) walls: Vec<MaterialRule>,
    pub(super) items: Vec<MaterialRule>,
}

pub(super) type LayerNames = BTreeMap<String, u8>;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum RuleSetDef {
    Flat(Vec<RuleDef>),
    Legacy(LegacyRuleSetDef),
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LegacyRuleSetDef {
    #[serde(default)]
    floors: Vec<MaterialRuleDef>,
    #[serde(default)]
    walls: Vec<MaterialRuleDef>,
    #[serde(default)]
    items: Vec<MaterialRuleDef>,
}

impl RuleSet {
    pub(super) fn from_def(def: RuleSetDef, layer_names: &LayerNames) -> Result<Self, String> {
        match def {
            RuleSetDef::Legacy(rules) => Ok(Self {
                floors: resolve_material_rules(rules.floors, layer_names, SurfaceScope::Cell)?,
                walls: resolve_material_rules(rules.walls, layer_names, SurfaceScope::Edge(WallRuleRelation::On))?,
                items: resolve_material_rules(rules.items, layer_names, SurfaceScope::Cell)?,
            }),
            RuleSetDef::Flat(rules) => {
                let mut floors = Vec::new();
                let mut walls = Vec::new();
                let mut items = Vec::new();
                for rule in rules {
                    if let Some(materials) = rule.floors.clone() {
                        floors.push(rule.surface_rule(materials, layer_names, SurfaceScope::Cell)?);
                    }
                    if let Some(materials) = rule.walls.clone() {
                        walls.push(rule.surface_rule(
                            materials,
                            layer_names,
                            SurfaceScope::Edge(WallRuleRelation::On),
                        )?);
                    }
                    if let Some(materials) = rule.touching_walls.clone() {
                        walls.push(rule.surface_rule(
                            materials,
                            layer_names,
                            SurfaceScope::Edge(WallRuleRelation::Touching),
                        )?);
                    }
                    if let Some(item_materials) = &rule.items {
                        for (item_type, material) in item_materials {
                            items.push(rule.item_rule(item_type.clone(), material.clone(), layer_names)?);
                        }
                    }
                }
                Ok(Self { floors, walls, items })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RuleDef {
    #[serde(default)]
    floors: Option<FaceMaterialDef>,
    #[serde(default)]
    walls: Option<FaceMaterialDef>,
    #[serde(default)]
    touching_walls: Option<FaceMaterialDef>,
    #[serde(default)]
    items: Option<BTreeMap<String, String>>,
    #[serde(default, alias = "level", alias = "layer", alias = "layers")]
    levels: Option<LayerRefs>,
    #[serde(flatten)]
    scope: CoordinateScopeDef,
    #[serde(default)]
    from: Option<[i32; 2]>,
    #[serde(default)]
    to: Option<[i32; 2]>,
}

impl RuleDef {
    fn surface_rule(
        &self,
        materials: FaceMaterialDef,
        layer_names: &LayerNames,
        scope: SurfaceScope,
    ) -> Result<MaterialRule, String> {
        let (cols, rows, wall_relation) = match scope {
            SurfaceScope::Cell => (self.scope.cell_cols(), self.scope.cell_rows(), WallRuleRelation::On),
            SurfaceScope::Edge(wall_relation) => (self.scope.edge_cols(), self.scope.edge_rows(), wall_relation),
        };
        Ok(MaterialRule {
            material: materials.all.clone(),
            materials: Some(materials),
            levels: resolve_level_scope(self.levels.as_ref(), layer_names)?,
            cols,
            rows,
            from: self.from,
            to: self.to,
            item_type: None,
            wall_relation,
        })
    }

    fn item_rule(&self, item_type: String, material: String, layer_names: &LayerNames) -> Result<MaterialRule, String> {
        Ok(MaterialRule {
            material: Some(material),
            materials: None,
            levels: resolve_level_scope(self.levels.as_ref(), layer_names)?,
            cols: self.scope.cell_cols(),
            rows: self.scope.cell_rows(),
            from: self.from,
            to: self.to,
            item_type: (item_type != "all").then_some(item_type),
            wall_relation: WallRuleRelation::On,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MaterialRuleDef {
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    materials: Option<FaceMaterialDef>,
    #[serde(default, alias = "level", alias = "layer", alias = "layers")]
    levels: Option<LayerRefs>,
    #[serde(flatten)]
    scope: CoordinateScopeDef,
    #[serde(default)]
    from: Option<[i32; 2]>,
    #[serde(default)]
    to: Option<[i32; 2]>,
    #[serde(default, rename = "type")]
    item_type: Option<String>,
}

impl MaterialRuleDef {
    fn into_rule(self, layer_names: &LayerNames, scope: SurfaceScope) -> Result<MaterialRule, String> {
        let (cols, rows, wall_relation) = match scope {
            SurfaceScope::Cell => (self.scope.cell_cols(), self.scope.cell_rows(), WallRuleRelation::On),
            SurfaceScope::Edge(wall_relation) => (self.scope.edge_cols(), self.scope.edge_rows(), wall_relation),
        };
        Ok(MaterialRule {
            material: self.material,
            materials: self.materials,
            levels: resolve_level_scope(self.levels.as_ref(), layer_names)?,
            cols,
            rows,
            from: self.from,
            to: self.to,
            item_type: self.item_type,
            wall_relation,
        })
    }
}

fn resolve_material_rules(
    rules: Vec<MaterialRuleDef>,
    layer_names: &LayerNames,
    scope: SurfaceScope,
) -> Result<Vec<MaterialRule>, String> {
    rules
        .into_iter()
        .map(|rule| rule.into_rule(layer_names, scope))
        .collect()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LayerRef {
    Number(u8),
    Name(String),
}

impl LayerRef {
    fn resolve(&self, layer_names: &LayerNames) -> Result<u8, String> {
        match self {
            Self::Number(level) => Ok(*level),
            Self::Name(name) => layer_names
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown material layer {name:?}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LayerRefs {
    One(LayerRef),
    Many(Vec<LayerRef>),
}

impl LayerRefs {
    fn resolve(&self, layer_names: &LayerNames) -> Result<Vec<u8>, String> {
        match self {
            Self::One(level) => Ok(vec![level.resolve(layer_names)?]),
            Self::Many(levels) => levels
                .iter()
                .map(|level| level.resolve(layer_names))
                .collect::<Result<Vec<_>, _>>(),
        }
    }
}

fn resolve_level_scope(levels: Option<&LayerRefs>, layer_names: &LayerNames) -> Result<Option<Vec<u8>>, String> {
    levels.map(|levels| levels.resolve(layer_names)).transpose()
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct CoordinateScopeDef {
    #[serde(default)]
    cols: Option<[i32; 2]>,
    #[serde(default)]
    rows: Option<[i32; 2]>,
    #[serde(default)]
    cell_cols: Option<[i32; 2]>,
    #[serde(default)]
    cell_rows: Option<[i32; 2]>,
    #[serde(default)]
    edge_cols: Option<[i32; 2]>,
    #[serde(default)]
    edge_rows: Option<[i32; 2]>,
}

impl CoordinateScopeDef {
    fn cell_cols(self) -> Option<[i32; 2]> {
        self.cell_cols.or(self.cols)
    }

    fn cell_rows(self) -> Option<[i32; 2]> {
        self.cell_rows.or(self.rows)
    }

    fn edge_cols(self) -> Option<[i32; 2]> {
        self.edge_cols.or(self.cols)
    }

    fn edge_rows(self) -> Option<[i32; 2]> {
        self.edge_rows.or(self.rows)
    }
}

#[derive(Debug, Clone)]
pub(super) struct MaterialRule {
    material: Option<String>,
    materials: Option<FaceMaterialDef>,
    levels: Option<Vec<u8>>,
    cols: Option<[i32; 2]>,
    rows: Option<[i32; 2]>,
    from: Option<[i32; 2]>,
    to: Option<[i32; 2]>,
    pub(super) item_type: Option<String>,
    wall_relation: WallRuleRelation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum WallRuleRelation {
    #[default]
    On,
    Touching,
}

#[derive(Debug, Clone, Copy)]
enum SurfaceScope {
    Cell,
    Edge(WallRuleRelation),
}

#[derive(Debug, Clone, Deserialize)]
struct FaceMaterialDef {
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
        self.levels.as_ref().is_none_or(|levels| levels.contains(&level))
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
        if let Some(levels) = &self.levels {
            score += if levels.len() == 1 {
                1_000
            } else {
                900_u16.saturating_sub(u16::try_from(levels.len()).unwrap_or(u16::MAX))
            };
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
