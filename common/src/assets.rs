use std::{fs, path::Path};

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
    pub fn material_for_floor(&self, floor: &Floor) -> &str {
        let mut material = None;
        for (col, row) in floor_cells(floor) {
            let next = self.material_for_floor_cell(floor.level, col, row);
            if let Some(current) = material {
                assert_eq!(
                    current, next,
                    "floor segment at level {} spans multiple materials: {current:?} and {next:?}",
                    floor.level
                );
            }
            material = Some(next);
        }

        material.unwrap_or_else(|| {
            let col = world_x_to_cell_col(f32::midpoint(floor.x1, floor.x2));
            let row = world_z_to_cell_row(f32::midpoint(floor.z1, floor.z2));
            self.material_for_floor_cell(floor.level, col, row)
        })
    }

    #[must_use]
    pub fn material_for_wall(&self, wall: &Wall) -> &str {
        let mut material = None;
        for (from, to) in wall_edges(wall) {
            let next = self.material_for_wall_edge(wall.level, from, to);
            if let Some(current) = material {
                assert_eq!(
                    current, next,
                    "wall segment at level {} spans multiple materials: {current:?} and {next:?}",
                    wall.level
                );
            }
            material = Some(next);
        }

        material.unwrap_or_else(|| {
            self.material_for_wall_edge(
                wall.level,
                [world_x_to_grid_col(wall.x1), world_z_to_grid_row(wall.z1)],
                [world_x_to_grid_col(wall.x2), world_z_to_grid_row(wall.z2)],
            )
        })
    }

    #[must_use]
    pub fn material_for_wall_edge(&self, level: u8, from: [i32; 2], to: [i32; 2]) -> &str {
        resolve_material(&self.material_rules.walls, |rule| {
            rule.matches_level(level) && rule.matches_edge(from, to)
        })
    }

    #[must_use]
    pub fn material_for_ramp_top(&self, ramp: &Ramp) -> &str {
        let lower_level = ramp_lower_level(ramp);
        resolve_material(&self.material_rules.ramp_tops, |rule| rule.matches_level(lower_level))
    }

    #[must_use]
    pub fn material_for_ramp_side(&self, ramp: &Ramp) -> &str {
        let lower_level = ramp_lower_level(ramp);
        resolve_material(&self.material_rules.ramp_sides, |rule| rule.matches_level(lower_level))
    }

    #[must_use]
    pub fn material_for_item(&self, item_type: ItemType) -> &str {
        let item_type_name = item_type_name(item_type);
        resolve_material(&self.material_rules.items, |rule| {
            rule.item_type
                .as_deref()
                .is_none_or(|rule_type| rule_type == item_type_name)
        })
    }

    #[must_use]
    fn material_for_floor_cell(&self, level: u8, col: i32, row: i32) -> &str {
        resolve_material(&self.material_rules.floors, |rule| {
            rule.matches_level(level) && rule.matches_cell(col, row)
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialRules {
    #[serde(default)]
    floors: Vec<MaterialRule>,
    #[serde(default)]
    walls: Vec<MaterialRule>,
    #[serde(default)]
    ramp_tops: Vec<MaterialRule>,
    #[serde(default)]
    ramp_sides: Vec<MaterialRule>,
    #[serde(default)]
    items: Vec<MaterialRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct MaterialRule {
    material: String,
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
        match (self.from, self.to) {
            (Some(rule_from), Some(rule_to)) => same_edge(rule_from, rule_to, from, to),
            (None, None) => true,
            _ => false,
        }
    }

    fn specificity(&self) -> u8 {
        u8::from(self.level.is_some() || self.levels.is_some())
            + u8::from(self.cols.is_some())
            + u8::from(self.rows.is_some())
            + u8::from(self.from.is_some() && self.to.is_some()) * 2
            + u8::from(self.item_type.is_some())
    }
}

fn resolve_material(rules: &[MaterialRule], matches: impl Fn(&MaterialRule) -> bool) -> &str {
    let mut best: Option<(&MaterialRule, u8)> = None;
    for rule in rules.iter().filter(|rule| matches(rule)) {
        let specificity = rule.specificity();
        match best {
            None => best = Some((rule, specificity)),
            Some((_, best_specificity)) if specificity > best_specificity => {
                best = Some((rule, specificity));
            }
            Some((best_rule, best_specificity))
                if specificity == best_specificity && rule.material != best_rule.material =>
            {
                panic!(
                    "conflicting asset material rules with same specificity: {:?} and {:?}",
                    best_rule.material, rule.material
                );
            }
            Some(_) => {}
        }
    }

    best.map(|(rule, _)| rule.material.as_str())
        .expect("asset material rule list must have a fallback rule")
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
