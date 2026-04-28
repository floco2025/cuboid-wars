use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Deserializer, de};

use super::{
    floors,
    lights::generate_wall_lights,
    mask::{Mask, mark_has_floor, mark_has_floor_above},
    ramps, walls,
};
use crate::{
    constants::FLOOR_OVERLAP,
    resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnField},
};
use common::{
    constants::*,
    protocol::{Floor, MapLayout, Wall},
};

const SUPPORTED_VERSION: u32 = 1;

// ============================================================================
// JSON schema
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct MapFile {
    pub version: u32,
    pub map: MapDef,
}

#[derive(Debug, Deserialize)]
pub struct MapDef {
    pub grid_cols: i32,
    pub grid_rows: i32,
    #[serde(default)]
    pub player_spawn_fields: Vec<PlayerSpawnDef>,
    pub levels: Vec<LevelDef>,
    #[serde(default)]
    pub ramps: Vec<RampDef>,
}

#[derive(Debug, Deserialize)]
pub struct LevelDef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub floors: Vec<[i32; 2]>,
    #[serde(default)]
    pub walls: Vec<[i32; 4]>,
}

#[derive(Debug, Deserialize)]
pub struct RampDef {
    pub low: [i32; 2],
    pub high: [i32; 2],
    pub lower_level: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerSpawnDef {
    pub level: u32,
    pub col: i32,
    pub row: i32,
}

impl PlayerSpawnDef {
    const fn legacy(col: i32, row: i32) -> Self {
        Self { level: 0, col, row }
    }

    const fn point(self) -> [i32; 2] {
        [self.col, self.row]
    }
}

impl<'de> Deserialize<'de> for PlayerSpawnDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<i32>::deserialize(deserializer)?;
        match values.as_slice() {
            [col, row] => Ok(Self::legacy(*col, *row)),
            [level, col, row] if *level >= 0 => Ok(Self {
                level: u32::try_from(*level).expect("nonnegative i32 should fit in u32"),
                col: *col,
                row: *row,
            }),
            [level, ..] if *level < 0 => Err(de::Error::custom("spawn field level must be nonnegative")),
            _ => Err(de::Error::custom("spawn field must be [col, row] or [level, col, row]")),
        }
    }
}

// ============================================================================
// Loading + validation
// ============================================================================

pub fn load_map(path: &Path) -> Result<MapDef> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let mut file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    validate_file(&file).with_context(|| format!("validating map at {}", path.display()))?;
    canonicalize(&mut file.map);
    Ok(file.map)
}

fn validate_file(file: &MapFile) -> Result<()> {
    if file.version != SUPPORTED_VERSION {
        return Err(anyhow!(
            "unsupported map file version {} (expected {})",
            file.version,
            SUPPORTED_VERSION
        ));
    }
    validate_map(&file.map)
}

fn validate_map(map_def: &MapDef) -> Result<()> {
    if map_def.grid_cols <= 0 || map_def.grid_rows <= 0 {
        return Err(anyhow!("grid_cols and grid_rows must be positive"));
    }
    if map_def.levels.is_empty() {
        return Err(anyhow!("at least one level is required"));
    }
    if map_def.player_spawn_fields.is_empty() {
        return Err(anyhow!("at least one player_spawn_fields entry is required"));
    }

    let mut spawn_fields = BTreeSet::new();
    for (spawn_idx, field) in map_def.player_spawn_fields.iter().enumerate() {
        if field.level as usize >= map_def.levels.len() {
            return Err(anyhow!(
                "player_spawn_fields[{spawn_idx}] level {} out of range (level count = {})",
                field.level,
                map_def.levels.len()
            ));
        }
        validate_floor(field.point(), map_def.grid_cols, map_def.grid_rows)
            .with_context(|| format!("player_spawn_fields[{spawn_idx}]"))?;
        if !spawn_fields.insert(*field) {
            return Err(anyhow!("duplicate player_spawn_fields {:?}", field));
        }
    }

    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let label = level_label(level_idx, level);
        if level.floors.is_empty() {
            return Err(anyhow!("{label}: at least one floor is required"));
        }

        let mut floors = BTreeSet::new();
        for (floor_idx, floor) in level.floors.iter().enumerate() {
            validate_floor(*floor, map_def.grid_cols, map_def.grid_rows)
                .with_context(|| format!("{label}: floors[{floor_idx}]"))?;
            if !floors.insert(*floor) {
                return Err(anyhow!("{label}: duplicate floor {:?}", floor));
            }
        }

        let mut walls_seen = BTreeSet::new();
        for (wall_idx, wall) in level.walls.iter().enumerate() {
            validate_wall(*wall, map_def.grid_cols, map_def.grid_rows)
                .with_context(|| format!("{label}: walls[{wall_idx}]"))?;
            let wall = normalized_wall(*wall);
            if !walls_seen.insert(wall) {
                return Err(anyhow!("{label}: duplicate wall {:?}", wall));
            }
        }
    }

    let floor_sets: Vec<BTreeSet<[i32; 2]>> = map_def
        .levels
        .iter()
        .map(|level| level.floors.iter().copied().collect())
        .collect();
    for field in &spawn_fields {
        if !floor_sets[field.level as usize].contains(&field.point()) {
            return Err(anyhow!(
                "player_spawn_fields {:?} is not a floor on level {}",
                field.point(),
                field.level
            ));
        }
    }

    let mut ramps_seen = BTreeSet::new();
    for (idx, ramp) in map_def.ramps.iter().enumerate() {
        validate_ramp(ramp, map_def.grid_cols, map_def.grid_rows, map_def.levels.len())
            .with_context(|| format!("ramps[{idx}]"))?;
        let key = (ramp.lower_level, ramp.low, ramp.high);
        if !ramps_seen.insert(key) {
            return Err(anyhow!("duplicate ramp {:?}", key));
        }
        for cell in ramp_footprint_cells(ramp) {
            for level in [ramp.lower_level, ramp.lower_level + 1] {
                if spawn_fields.contains(&PlayerSpawnDef {
                    level,
                    col: cell[0],
                    row: cell[1],
                }) {
                    return Err(anyhow!(
                        "player_spawn_fields {:?} overlaps a ramp on level {}",
                        cell,
                        level
                    ));
                }
            }
        }
    }

    Ok(())
}

fn level_label(level_idx: usize, level: &LevelDef) -> String {
    match &level.name {
        Some(name) if !name.is_empty() => format!("level {level_idx} ({name})"),
        _ => format!("level {level_idx}"),
    }
}

fn validate_floor(floor: [i32; 2], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [col, row] = floor;
    if col < 0 || col >= grid_cols || row < 0 || row >= grid_rows {
        return Err(anyhow!(
            "floor {:?} out of grid bounds {}x{}",
            floor,
            grid_cols,
            grid_rows
        ));
    }
    Ok(())
}

fn validate_grid_point(point: [i32; 2], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [col, row] = point;
    if col < 0 || col > grid_cols || row < 0 || row > grid_rows {
        return Err(anyhow!(
            "grid point {:?} out of grid-line bounds {}x{}",
            point,
            grid_cols,
            grid_rows
        ));
    }
    Ok(())
}

fn validate_wall(wall: [i32; 4], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [c0, r0, c1, r1] = wall;
    validate_grid_point([c0, r0], grid_cols, grid_rows)?;
    validate_grid_point([c1, r1], grid_cols, grid_rows)?;

    let length = (c1 - c0).abs() + (r1 - r0).abs();
    if length != 1 {
        return Err(anyhow!("wall {:?} must be exactly one grid edge", wall));
    }
    Ok(())
}

fn validate_ramp(ramp: &RampDef, grid_cols: i32, grid_rows: i32, level_count: usize) -> Result<()> {
    if ramp.lower_level as usize + 1 >= level_count {
        return Err(anyhow!(
            "lower_level {} has no upper level (level count = {})",
            ramp.lower_level,
            level_count
        ));
    }
    validate_grid_point(ramp.low, grid_cols, grid_rows).context("low")?;
    validate_grid_point(ramp.high, grid_cols, grid_rows).context("high")?;

    let width = (ramp.high[0] - ramp.low[0]).abs();
    let height = (ramp.high[1] - ramp.low[1]).abs();
    if width == 0 || height == 0 {
        return Err(anyhow!("low/high must span a non-empty rectangular footprint"));
    }
    if width == height {
        return Err(anyhow!("ramp footprint must have one clear longer axis"));
    }
    Ok(())
}

fn ramp_footprint_cells(ramp: &RampDef) -> Vec<[i32; 2]> {
    let col_min = ramp.low[0].min(ramp.high[0]);
    let col_max = ramp.low[0].max(ramp.high[0]);
    let row_min = ramp.low[1].min(ramp.high[1]);
    let row_max = ramp.low[1].max(ramp.high[1]);

    let mut cells = Vec::new();
    for row in row_min..row_max {
        for col in col_min..col_max {
            cells.push([col, row]);
        }
    }
    cells
}

fn canonicalize(map_def: &mut MapDef) {
    map_def
        .player_spawn_fields
        .sort_by_key(|field| (field.level, field.row, field.col));
    map_def.player_spawn_fields.dedup();

    for level in &mut map_def.levels {
        level.floors.sort_by_key(|[col, row]| (*row, *col));
        level.floors.dedup();

        for wall in &mut level.walls {
            *wall = normalized_wall(*wall);
        }
        level.walls.sort();
        level.walls.dedup();
    }

    map_def.ramps.sort_by_key(|r| (r.lower_level, r.low, r.high));
    map_def.ramps.dedup_by_key(|r| (r.lower_level, r.low, r.high));
}

fn normalized_wall(wall: [i32; 4]) -> [i32; 4] {
    let [c0, r0, c1, r1] = wall;
    if (c1, r1) < (c0, r0) { [c1, r1, c0, r0] } else { wall }
}

// ============================================================================
// Compile map source -> MapLayout + MapConfig
// ============================================================================

#[must_use]
pub fn compile_map(map_def: &MapDef) -> (MapLayout, MapConfig) {
    let cols = map_def.grid_cols;
    let rows = map_def.grid_rows;

    let ramp_specs: Vec<ramps::RampSpec> = map_def.ramps.iter().map(ramp_spec_from_def).collect();

    let masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| {
            let mut m = empty_mask(cols, rows);
            for [col, row] in &level.floors {
                m[*row as usize][*col as usize] = true;
            }
            m
        })
        .collect();

    let mut level_grids: Vec<LevelGrid> = map_def
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, level)| {
            let mut cell_grid = CellGrid::new(cols, rows);
            let mut edge_grid = EdgeGrid::new(cols, rows);
            mark_has_floor(&mut cell_grid, &masks[level_idx]);
            for wall in &level.walls {
                set_wall_edge(&mut edge_grid, *wall);
            }
            LevelGrid {
                cells: cell_grid,
                edges: edge_grid,
            }
        })
        .collect();

    // Apply ramp flags to each lower-level cell grid. Spawn selection skips
    // these cells on any level.
    for (level_idx, level_grid) in level_grids.iter_mut().enumerate() {
        let level_u32 = u32::try_from(level_idx).unwrap_or(u32::MAX);
        ramps::apply_to_level_cells(&mut level_grid.cells, &ramp_specs, level_u32);
    }
    for level_idx in 0..level_grids.len().saturating_sub(1) {
        mark_has_floor_above(&mut level_grids[level_idx].cells, &masks[level_idx + 1]);
    }

    let mut wall_lights = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        wall_lights.extend(generate_wall_lights(level_grid, level_u8));
    }

    let mut all_walls: Vec<Wall> = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut tier = walls::generate_walls(&level_grid.edges, cols, rows, level_u8);
        tier = walls::merge_walls(tier);
        all_walls.extend(tier);
    }

    let mut all_floors: Vec<Floor> = Vec::new();
    for (level_idx, m) in masks.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let y = f32::from(level_u8) * LEVEL_HEIGHT;
        let mut tier = floors::emit_floor_tier(m, cols, rows, level_u8, y);
        if !FLOOR_OVERLAP {
            tier = floors::merge_floors(tier);
        }
        all_floors.extend(tier);
    }

    let map_layout = MapLayout {
        walls: all_walls,
        ramps: ramps::specs_to_ramps(&ramp_specs),
        wall_lights,
        floors: all_floors,
    };

    (
        map_layout,
        MapConfig {
            levels: level_grids,
            player_spawn_fields: player_spawn_fields(map_def),
        },
    )
}

fn ramp_spec_from_def(r: &RampDef) -> ramps::RampSpec {
    ramps::RampSpec {
        lower_level: r.lower_level,
        low: r.low,
        high: r.high,
    }
}

fn empty_mask(grid_cols: i32, grid_rows: i32) -> Mask {
    vec![vec![false; grid_cols as usize]; grid_rows as usize]
}

fn player_spawn_fields(map_def: &MapDef) -> Vec<PlayerSpawnField> {
    map_def
        .player_spawn_fields
        .iter()
        .map(|field| PlayerSpawnField {
            level: u8::try_from(field.level).unwrap_or(u8::MAX),
            col: field.col,
            row: field.row,
        })
        .collect()
}

fn set_wall_edge(edges: &mut EdgeGrid, wall: [i32; 4]) {
    let [c0, r0, c1, r1] = wall;
    if r0 == r1 {
        edges.horizontal[r0 as usize][c0.min(c1) as usize] = true;
    } else {
        edges.vertical[r0.min(r1) as usize][c0 as usize] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(floors: Vec<[i32; 2]>) -> LevelDef {
        LevelDef {
            name: None,
            floors,
            walls: Vec::new(),
        }
    }

    const fn spawn(level: u32, col: i32, row: i32) -> PlayerSpawnDef {
        PlayerSpawnDef { level, col, row }
    }

    #[test]
    fn validation_rejects_spawn_field_without_floor_on_its_level() {
        let map_def = MapDef {
            grid_cols: 4,
            grid_rows: 4,
            player_spawn_fields: vec![spawn(1, 0, 0)],
            levels: vec![level(vec![[0, 0]]), level(vec![[1, 0]])],
            ramps: Vec::new(),
        };

        let err = validate_map(&map_def).expect_err("spawn field must be a floor on its level");
        assert!(err.to_string().contains("not a floor on level 1"));
    }

    #[test]
    fn validation_accepts_spawn_field_on_higher_level_floor() {
        let map_def = MapDef {
            grid_cols: 4,
            grid_rows: 4,
            player_spawn_fields: vec![spawn(1, 0, 0)],
            levels: vec![level(vec![[1, 0]]), level(vec![[0, 0]])],
            ramps: Vec::new(),
        };

        validate_map(&map_def).expect("spawn field should be allowed on any level floor");
    }

    #[test]
    fn validation_rejects_spawn_field_on_same_level_ramp() {
        let map_def = MapDef {
            grid_cols: 4,
            grid_rows: 4,
            player_spawn_fields: vec![spawn(1, 0, 0)],
            levels: vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
            ramps: vec![RampDef {
                low: [0, 0],
                high: [1, 2],
                lower_level: 1,
            }],
        };

        let err = validate_map(&map_def).expect_err("spawn field must not overlap ramp footprint");
        assert!(err.to_string().contains("overlaps a ramp on level 1"));
    }
}
