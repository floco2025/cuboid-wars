use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{
    constants::{GRID_CELL_SIZE, GRID_COLS, GRID_ROWS, LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    protocol::{Floor, ItemType, Ramp, Wall},
};

#[derive(Debug, Clone, Deserialize)]
pub struct AssetRules {
    pub material_rules: MaterialRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalMaterials {
    pub north: String,
    pub south: String,
    pub east: String,
    pub west: String,
    pub top: String,
    pub bottom: String,
}

impl DirectionalMaterials {
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

impl AssetRules {
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
    pub fn materials_for_floor(&self, floor: &Floor) -> DirectionalMaterials {
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
    pub fn material_for_floor(&self, floor: &Floor) -> String {
        self.materials_for_floor(floor).top
    }

    #[must_use]
    pub fn materials_for_wall(&self, wall: &Wall) -> DirectionalMaterials {
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
    pub fn material_for_wall(&self, wall: &Wall) -> String {
        self.materials_for_wall(wall).first().to_owned()
    }

    #[must_use]
    pub fn materials_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> DirectionalMaterials {
        resolve_directional_materials(&self.material_rules.walls, |rule| {
            rule.matches_level(level) && rule.matches_edge(from, to)
        })
    }

    #[must_use]
    pub fn material_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> String {
        self.materials_for_wall_edge(level, from, to).first().to_owned()
    }

    #[must_use]
    pub fn materials_for_ramp_top(&self, ramp: &Ramp) -> DirectionalMaterials {
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
    pub fn material_for_ramp_top(&self, ramp: &Ramp) -> String {
        self.materials_for_ramp_top(ramp).top
    }

    #[must_use]
    pub fn materials_for_ramp_side(&self, ramp: &Ramp) -> DirectionalMaterials {
        let lower_level = ramp_lower_level(ramp);
        self.materials_for_interior_wall(lower_level)
    }

    #[must_use]
    pub fn material_for_ramp_side(&self, ramp: &Ramp) -> String {
        self.materials_for_ramp_side(ramp).first().to_owned()
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
    fn materials_for_floor_cell(&self, level: u8, col: i32, row: i32) -> DirectionalMaterials {
        resolve_directional_materials(&self.material_rules.floors, |rule| {
            rule.matches_level(level) && rule.matches_cell(col, row)
        })
    }

    #[must_use]
    fn materials_for_interior_wall(&self, level: u8) -> DirectionalMaterials {
        resolve_directional_materials(&self.material_rules.walls, |rule| {
            rule.matches_level(level) && !rule.has_edge_scope()
        })
    }
}

#[derive(Debug, Clone)]
pub struct MaterialRules {
    floors: Vec<MaterialRule>,
    walls: Vec<MaterialRule>,
    items: Vec<MaterialRule>,
}

impl<'de> Deserialize<'de> for MaterialRules {
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
    floors: Option<DirectionalMaterialRule>,
    #[serde(default)]
    walls: Option<DirectionalMaterialRule>,
    #[serde(default)]
    touching_walls: Option<DirectionalMaterialRule>,
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
    fn material_rule(&self, materials: DirectionalMaterialRule, wall_relation: WallRuleRelation) -> MaterialRule {
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
struct MaterialRule {
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    materials: Option<DirectionalMaterialRule>,
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
    item_type: Option<String>,
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
struct DirectionalMaterialRule {
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
    fn matches_level(&self, level: u8) -> bool {
        self.level.is_none_or(|rule_level| rule_level == level)
            && self.levels.as_ref().is_none_or(|levels| levels.contains(&level))
    }

    fn matches_cell(&self, col: i32, row: i32) -> bool {
        self.cols.is_none_or(|[min, max]| (min..=max).contains(&col))
            && self.rows.is_none_or(|[min, max]| (min..=max).contains(&row))
    }

    fn matches_edge(&self, from: [i32; 2], to: [i32; 2]) -> bool {
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

    fn specificity(&self) -> u16 {
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

    fn has_edge_scope(&self) -> bool {
        self.cols.is_some() || self.rows.is_some() || self.from.is_some() || self.to.is_some()
    }

    fn directional_materials(&self) -> DirectionalMaterials {
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

        DirectionalMaterials {
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

    fn item_material(&self) -> &str {
        self.material
            .as_deref()
            .expect("asset item material rule must define `material`")
    }
}

fn range_specificity([min, max]: [i32; 2], full_len: i32) -> u16 {
    let len = (max - min + 1).clamp(1, full_len);
    100 + u16::try_from(full_len - len).expect("grid range length should fit in u16")
}

fn resolve_directional_materials(
    rules: &[MaterialRule],
    matches: impl Fn(&MaterialRule) -> bool,
) -> DirectionalMaterials {
    let mut best: Option<(&MaterialRule, u16)> = None;
    for rule in rules.iter().filter(|rule| matches(rule)) {
        let specificity = rule.specificity();
        match best {
            None => best = Some((rule, specificity)),
            Some((_, best_specificity)) if specificity > best_specificity => {
                best = Some((rule, specificity));
            }
            Some((best_rule, best_specificity)) if specificity == best_specificity => {
                let best_materials = best_rule.directional_materials();
                let rule_materials = rule.directional_materials();
                if best_materials == rule_materials {
                    continue;
                }
                panic!(
                    "conflicting asset material rules with same specificity: {:?} and {:?}",
                    best_materials, rule_materials
                );
            }
            Some(_) => {}
        }
    }

    best.map(|(rule, _)| rule.directional_materials())
        .expect("asset material rule list must have a fallback rule")
}

fn resolve_item_material(rules: &[MaterialRule], matches: impl Fn(&MaterialRule) -> bool) -> &str {
    let mut best: Option<(&MaterialRule, u16)> = None;
    for rule in rules.iter().filter(|rule| matches(rule)) {
        let specificity = rule.specificity();
        match best {
            None => best = Some((rule, specificity)),
            Some((_, best_specificity)) if specificity > best_specificity => {
                best = Some((rule, specificity));
            }
            Some((best_rule, best_specificity))
                if specificity == best_specificity && rule.item_material() != best_rule.item_material() =>
            {
                panic!(
                    "conflicting asset item material rules with same specificity: {:?} and {:?}",
                    best_rule.item_material(),
                    rule.item_material()
                );
            }
            Some(_) => {}
        }
    }

    best.map(|(rule, _)| rule.item_material())
        .expect("asset item material rule list must have a fallback rule")
}

fn floor_cells(floor: &Floor) -> Vec<(i32, i32)> {
    let min_x = floor.x1.min(floor.x2);
    let max_x = floor.x1.max(floor.x2);
    let min_z = floor.z1.min(floor.z2);
    let max_z = floor.z1.max(floor.z2);

    let min_col = first_cell_center_at_or_after(min_x, MAP_WIDTH, GRID_COLS);
    let max_col = last_cell_center_at_or_before(max_x, MAP_WIDTH, GRID_COLS);
    let min_row = first_cell_center_at_or_after(min_z, MAP_DEPTH, GRID_ROWS);
    let max_row = last_cell_center_at_or_before(max_z, MAP_DEPTH, GRID_ROWS);

    let mut cells = Vec::new();
    if min_col > max_col || min_row > max_row {
        return cells;
    }
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            cells.push((col, row));
        }
    }
    cells
}

fn ramp_cells(ramp: &Ramp) -> Vec<(i32, i32)> {
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    let min_col = first_cell_center_at_or_after(min_x, MAP_WIDTH, GRID_COLS);
    let max_col = last_cell_center_at_or_before(max_x, MAP_WIDTH, GRID_COLS);
    let min_row = first_cell_center_at_or_after(min_z, MAP_DEPTH, GRID_ROWS);
    let max_row = last_cell_center_at_or_before(max_z, MAP_DEPTH, GRID_ROWS);

    let mut cells = Vec::new();
    if min_col > max_col || min_row > max_row {
        return cells;
    }
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            cells.push((col, row));
        }
    }
    cells
}

fn wall_edges(wall: &Wall) -> Vec<([i32; 2], [i32; 2])> {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    if dx >= dz {
        let row = world_z_to_grid_row(f32::midpoint(wall.z1, wall.z2));
        let min_col = first_edge_midpoint_at_or_after(wall.x1.min(wall.x2), MAP_WIDTH, GRID_COLS);
        let max_col = last_edge_midpoint_at_or_before(wall.x1.max(wall.x2), MAP_WIDTH, GRID_COLS);
        if min_col > max_col {
            return Vec::new();
        }
        return (min_col..=max_col).map(|col| ([col, row], [col + 1, row])).collect();
    }

    let col = world_x_to_grid_col(f32::midpoint(wall.x1, wall.x2));
    let min_row = first_edge_midpoint_at_or_after(wall.z1.min(wall.z2), MAP_DEPTH, GRID_ROWS);
    let max_row = last_edge_midpoint_at_or_before(wall.z1.max(wall.z2), MAP_DEPTH, GRID_ROWS);
    if min_row > max_row {
        return Vec::new();
    }
    (min_row..=max_row).map(|row| ([col, row], [col, row + 1])).collect()
}

fn first_cell_center_at_or_after(world: f32, map_size: f32, count: i32) -> i32 {
    (((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).ceil() as i32).clamp(0, count - 1)
}

fn last_cell_center_at_or_before(world: f32, map_size: f32, count: i32) -> i32 {
    (((world + map_size / 2.0 - GRID_CELL_SIZE / 2.0) / GRID_CELL_SIZE).floor() as i32).clamp(0, count - 1)
}

fn first_edge_midpoint_at_or_after(world: f32, map_size: f32, edge_count: i32) -> i32 {
    first_cell_center_at_or_after(world, map_size, edge_count)
}

fn last_edge_midpoint_at_or_before(world: f32, map_size: f32, edge_count: i32) -> i32 {
    last_cell_center_at_or_before(world, map_size, edge_count)
}

#[must_use]
pub fn world_x_to_grid_col(x: f32) -> i32 {
    ((x + MAP_WIDTH / 2.0) / GRID_CELL_SIZE).round() as i32
}

#[must_use]
pub fn world_z_to_grid_row(z: f32) -> i32 {
    ((z + MAP_DEPTH / 2.0) / GRID_CELL_SIZE).round() as i32
}

fn world_x_to_cell_col(x: f32) -> i32 {
    ((x + MAP_WIDTH / 2.0) / GRID_CELL_SIZE)
        .floor()
        .clamp(0.0, (GRID_COLS - 1) as f32) as i32
}

fn world_z_to_cell_row(z: f32) -> i32 {
    ((z + MAP_DEPTH / 2.0) / GRID_CELL_SIZE)
        .floor()
        .clamp(0.0, (GRID_ROWS - 1) as f32) as i32
}

#[must_use]
pub fn grid_col_to_world_x(col: i32) -> f32 {
    (col as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0))
}

#[must_use]
pub fn grid_row_to_world_z(row: i32) -> f32 {
    (row as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0))
}

fn ramp_lower_level(ramp: &Ramp) -> u8 {
    let lower_y = ramp.y1.min(ramp.y2);
    (lower_y / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8
}

fn item_type_name(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::SpeedPowerUp => "SpeedPowerUp",
        ItemType::MultiShotPowerUp => "MultiShotPowerUp",
        ItemType::PhasingPowerUp => "PhasingPowerUp",
        ItemType::Cookie => "Cookie",
    }
}

fn same_edge(rule_from: [i32; 2], rule_to: [i32; 2], from: [i32; 2], to: [i32; 2]) -> bool {
    (rule_from == from && rule_to == to) || (rule_from == to && rule_to == from)
}

fn touches_vertical_line(from: [i32; 2], to: [i32; 2], col: i32, [row_min, row_max]: [i32; 2]) -> bool {
    let is_horizontal_edge = from[1] == to[1] && from[0] != to[0];
    let row = from[1];
    is_horizontal_edge
        && (row_min..=row_max).contains(&row)
        && (from[0] == col || to[0] == col)
        && !(from[0] == col && to[0] == col)
}

fn touches_horizontal_line(from: [i32; 2], to: [i32; 2], row: i32, [col_min, col_max]: [i32; 2]) -> bool {
    let is_vertical_edge = from[0] == to[0] && from[1] != to[1];
    let col = from[0];
    is_vertical_edge
        && (col_min..=col_max).contains(&col)
        && (from[1] == row || to[1] == row)
        && !(from[1] == row && to[1] == row)
}

fn touches_rectangle(from: [i32; 2], to: [i32; 2], [col_min, col_max]: [i32; 2], [row_min, row_max]: [i32; 2]) -> bool {
    let min_col = from[0].min(to[0]);
    let max_col = from[0].max(to[0]);
    let min_row = from[1].min(to[1]);
    let max_row = from[1].max(to[1]);

    if from[1] == to[1] {
        let row = from[1];
        return (row_min..=row_max).contains(&row) && (max_col == col_min || min_col == col_max);
    }

    if from[0] == to[0] {
        let col = from[0];
        return (col_min..=col_max).contains(&col) && (max_row == row_min || min_row == row_max);
    }

    false
}
