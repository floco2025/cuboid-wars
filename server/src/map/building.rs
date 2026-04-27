use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use super::{
    floors,
    lights::generate_wall_lights,
    mask::{Mask, mark_has_floor, mark_has_floor_above},
    ramps, walls,
};
use crate::{
    constants::FLOOR_OVERLAP,
    resources::{GridCell, GridConfig},
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
pub struct BuildingFile {
    pub version: u32,
    pub building: BuildingDef,
}

#[derive(Debug, Deserialize)]
pub struct BuildingDef {
    pub grid_cols: i32,
    pub grid_rows: i32,
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

// ============================================================================
// Loading + validation
// ============================================================================

pub fn load_building(path: &Path) -> Result<BuildingDef> {
    let text = fs::read_to_string(path).with_context(|| format!("reading building at {}", path.display()))?;
    let mut file: BuildingFile =
        serde_json::from_str(&text).with_context(|| format!("parsing building JSON at {}", path.display()))?;
    validate_file(&file).with_context(|| format!("validating building at {}", path.display()))?;
    canonicalize(&mut file.building);
    Ok(file.building)
}

fn validate_file(file: &BuildingFile) -> Result<()> {
    if file.version != SUPPORTED_VERSION {
        return Err(anyhow!(
            "unsupported building file version {} (expected {})",
            file.version,
            SUPPORTED_VERSION
        ));
    }
    validate_building(&file.building)
}

fn validate_building(b: &BuildingDef) -> Result<()> {
    if b.grid_cols <= 0 || b.grid_rows <= 0 {
        return Err(anyhow!("grid_cols and grid_rows must be positive"));
    }
    if b.levels.is_empty() {
        return Err(anyhow!("at least one level is required"));
    }

    for (level_idx, level) in b.levels.iter().enumerate() {
        let label = level_label(level_idx, level);
        if level.floors.is_empty() {
            return Err(anyhow!("{label}: at least one floor is required"));
        }

        let mut floors = BTreeSet::new();
        for (floor_idx, floor) in level.floors.iter().enumerate() {
            validate_floor(*floor, b.grid_cols, b.grid_rows)
                .with_context(|| format!("{label}: floors[{floor_idx}]"))?;
            if !floors.insert(*floor) {
                return Err(anyhow!("{label}: duplicate floor {:?}", floor));
            }
        }

        let mut walls_seen = BTreeSet::new();
        for (wall_idx, wall) in level.walls.iter().enumerate() {
            validate_wall(*wall, b.grid_cols, b.grid_rows).with_context(|| format!("{label}: walls[{wall_idx}]"))?;
            let wall = normalized_wall(*wall);
            if !walls_seen.insert(wall) {
                return Err(anyhow!("{label}: duplicate wall {:?}", wall));
            }
        }
    }

    let mut ramps_seen = BTreeSet::new();
    for (idx, ramp) in b.ramps.iter().enumerate() {
        validate_ramp(ramp, b.grid_cols, b.grid_rows, b.levels.len()).with_context(|| format!("ramps[{idx}]"))?;
        let key = (ramp.lower_level, ramp.low, ramp.high);
        if !ramps_seen.insert(key) {
            return Err(anyhow!("duplicate ramp {:?}", key));
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

fn canonicalize(b: &mut BuildingDef) {
    for level in &mut b.levels {
        level.floors.sort_by_key(|[col, row]| (*row, *col));
        level.floors.dedup();

        for wall in &mut level.walls {
            *wall = normalized_wall(*wall);
        }
        level.walls.sort();
        level.walls.dedup();
    }

    b.ramps.sort_by_key(|r| (r.lower_level, r.low, r.high));
    b.ramps.dedup_by_key(|r| (r.lower_level, r.low, r.high));
}

fn normalized_wall(wall: [i32; 4]) -> [i32; 4] {
    let [c0, r0, c1, r1] = wall;
    if (c1, r1) < (c0, r0) { [c1, r1, c0, r0] } else { wall }
}

// ============================================================================
// Compile building -> MapLayout + GridConfig
// ============================================================================

#[must_use]
pub fn compile_building(b: &BuildingDef) -> (MapLayout, GridConfig) {
    let cols = b.grid_cols;
    let rows = b.grid_rows;

    let ramp_specs: Vec<ramps::RampSpec> = b.ramps.iter().map(ramp_spec_from_def).collect();

    let masks: Vec<Mask> = b
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

    let mut level_grids: Vec<Vec<Vec<GridCell>>> = b
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, level)| {
            let mut grid = vec![vec![GridCell::default(); cols as usize]; rows as usize];
            mark_has_floor(&mut grid, &masks[level_idx]);
            for wall in &level.walls {
                set_wall_edge(&mut grid, *wall, cols, rows);
            }
            grid
        })
        .collect();

    // Apply ramp flags + has_floor_above to the level-0 grid (used by spawn
    // and wall-light placement).
    if let Some(grid0) = level_grids.get_mut(0) {
        ramps::apply_to_level0_grid(grid0, &ramp_specs);
    }
    if level_grids.len() > 1
        && let Some(grid0) = level_grids.get_mut(0)
    {
        mark_has_floor_above(grid0, &masks[1]);
    }

    let wall_lights = generate_wall_lights(&level_grids[0]);

    let mut all_walls: Vec<Wall> = Vec::new();
    for (level_idx, grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut tier = walls::generate_walls(grid, cols, rows, level_u8);
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

    let level0_grid = level_grids
        .into_iter()
        .next()
        .unwrap_or_else(|| vec![vec![GridCell::default(); cols as usize]; rows as usize]);

    (map_layout, GridConfig { grid: level0_grid })
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

fn set_wall_edge(grid: &mut [Vec<GridCell>], wall: [i32; 4], grid_cols: i32, grid_rows: i32) {
    let [c0, r0, c1, r1] = wall;
    if r0 == r1 {
        set_horizontal_wall(grid, r0, c0.min(c1), grid_rows);
    } else {
        set_vertical_wall(grid, r0.min(r1), c0, grid_cols);
    }
}

fn set_horizontal_wall(grid: &mut [Vec<GridCell>], row: i32, col: i32, grid_rows: i32) {
    if row == 0 {
        grid[0][col as usize].has_north_wall = true;
    } else if row == grid_rows {
        grid[(grid_rows - 1) as usize][col as usize].has_south_wall = true;
    } else {
        grid[row as usize][col as usize].has_north_wall = true;
    }
}

fn set_vertical_wall(grid: &mut [Vec<GridCell>], row: i32, col: i32, grid_cols: i32) {
    if col == 0 {
        grid[row as usize][0].has_west_wall = true;
    } else if col == grid_cols {
        grid[row as usize][(grid_cols - 1) as usize].has_east_wall = true;
    } else {
        grid[row as usize][col as usize].has_west_wall = true;
    }
}
