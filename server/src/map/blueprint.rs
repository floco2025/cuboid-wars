use std::fs;
use std::path::Path;

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

// ============================================================================
// TOML schema
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct BlueprintFile {
    pub building: BuildingBlueprint,
}

#[derive(Debug, Deserialize)]
pub struct BuildingBlueprint {
    pub grid_cols: i32,
    pub grid_rows: i32,
    pub num_levels: u32,
    pub levels: Vec<LevelBlueprint>,
    #[serde(default)]
    pub ramps: Vec<RampBlueprint>,
}

#[derive(Debug, Deserialize)]
pub struct LevelBlueprint {
    pub name: String,
    /// Floor rectangles, each `[col0, row0, col_end, row_end]` and half-open.
    /// Cells inside these rects have floor unless excluded by `voids` or a ramp
    /// body above them.
    pub floors: Vec<[i32; 4]>,
    /// Void rectangles, each `[col0, row0, col_end, row_end]` and half-open.
    #[serde(default)]
    pub voids: Vec<[i32; 4]>,
    /// Explicit wall segments on grid lines. Segment endpoints use grid-line
    /// coordinates, not cell coordinates. Horizontal walls have equal rows;
    /// vertical walls have equal cols. Doorways are represented by gaps between
    /// wall segments.
    #[serde(default)]
    pub walls: Vec<WallBlueprint>,
}

#[derive(Debug, Deserialize)]
pub struct WallBlueprint {
    pub from: [i32; 2],
    pub to: [i32; 2],
}

#[derive(Debug, Deserialize)]
pub struct RampBlueprint {
    /// Ramp footprint, `[col0, row0, col_end, row_end]`, half-open.
    pub rect: [i32; 4],
    /// Direction the ramp ascends: "north", "south", "east", or "west".
    pub up: String,
    pub lower_level: u32,
}

// ============================================================================
// Loading + validation
// ============================================================================

pub fn load_blueprint(path: &Path) -> Result<BuildingBlueprint> {
    let text = fs::read_to_string(path).with_context(|| format!("reading blueprint at {}", path.display()))?;
    let file: BlueprintFile =
        toml::from_str(&text).with_context(|| format!("parsing blueprint at {}", path.display()))?;
    validate(&file.building).with_context(|| format!("validating blueprint at {}", path.display()))?;
    Ok(file.building)
}

fn validate(b: &BuildingBlueprint) -> Result<()> {
    if b.grid_cols <= 0 || b.grid_rows <= 0 {
        return Err(anyhow!("grid_cols and grid_rows must be positive"));
    }
    if b.num_levels == 0 {
        return Err(anyhow!("num_levels must be >= 1"));
    }
    if b.levels.len() != b.num_levels as usize {
        return Err(anyhow!(
            "num_levels = {} but {} levels listed",
            b.num_levels,
            b.levels.len()
        ));
    }

    for (idx, level) in b.levels.iter().enumerate() {
        if level.floors.is_empty() {
            return Err(anyhow!(
                "level {} ({}): at least one floor rect is required",
                idx,
                level.name
            ));
        }
        for (floor_idx, floor) in level.floors.iter().enumerate() {
            validate_rect(floor, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): floors[{}]", idx, level.name, floor_idx))?;
        }
        for (void_idx, void) in level.voids.iter().enumerate() {
            validate_rect(void, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): voids[{}]", idx, level.name, void_idx))?;
            if !level.floors.iter().any(|floor| contains_rect(floor, void)) {
                return Err(anyhow!(
                    "level {} ({}): voids[{}] {:?} is not contained by any floor rect",
                    idx,
                    level.name,
                    void_idx,
                    void
                ));
            }
        }
        for (wall_idx, wall) in level.walls.iter().enumerate() {
            validate_wall(wall, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): walls[{}]", idx, level.name, wall_idx))?;
        }
    }

    for (idx, ramp) in b.ramps.iter().enumerate() {
        if (ramp.lower_level + 1) >= b.num_levels {
            return Err(anyhow!(
                "ramps[{}]: lower_level {} has no upper level (num_levels = {})",
                idx,
                ramp.lower_level,
                b.num_levels
            ));
        }
        validate_rect(&ramp.rect, b.grid_cols, b.grid_rows).with_context(|| format!("ramps[{}].rect", idx))?;
        match ramp.up.as_str() {
            "north" | "south" | "east" | "west" => {}
            other => return Err(anyhow!("ramps[{}]: unknown up direction {:?}", idx, other)),
        }
    }

    Ok(())
}

fn validate_rect(rect: &[i32; 4], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [c0, r0, c_end, r_end] = *rect;
    if c0 < 0 || r0 < 0 || c_end > grid_cols || r_end > grid_rows {
        return Err(anyhow!(
            "rect {:?} out of grid bounds {}x{}",
            rect,
            grid_cols,
            grid_rows
        ));
    }
    if c_end <= c0 || r_end <= r0 {
        return Err(anyhow!("rect {:?} is empty or inverted", rect));
    }
    Ok(())
}

fn contains_rect(outer: &[i32; 4], inner: &[i32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

fn validate_wall(wall: &WallBlueprint, grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [c0, r0] = wall.from;
    let [c1, r1] = wall.to;
    if c0 < 0 || c0 > grid_cols || c1 < 0 || c1 > grid_cols || r0 < 0 || r0 > grid_rows || r1 < 0 || r1 > grid_rows {
        return Err(anyhow!(
            "wall {:?}->{:?} out of grid-line bounds {}x{}",
            wall.from,
            wall.to,
            grid_cols,
            grid_rows
        ));
    }
    if c0 != c1 && r0 != r1 {
        return Err(anyhow!("wall {:?}->{:?} must be axis-aligned", wall.from, wall.to));
    }
    if c0 == c1 && r0 == r1 {
        return Err(anyhow!("wall {:?}->{:?} has zero length", wall.from, wall.to));
    }
    Ok(())
}

// ============================================================================
// Compile blueprint -> MapLayout + GridConfig
// ============================================================================

#[must_use]
pub fn compile_blueprint(b: &BuildingBlueprint) -> (MapLayout, GridConfig) {
    let cols = b.grid_cols;
    let rows = b.grid_rows;

    // Convert ramps.
    let ramp_specs: Vec<ramps::RampSpec> = b.ramps.iter().map(ramp_spec_from_blueprint).collect();

    // Per-level masks: floor rects minus voids minus ramp body cells.
    let masks: Vec<Mask> = b
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, lev)| {
            let mut m = empty_mask(cols, rows);
            for floor in &lev.floors {
                add_rect(&mut m, *floor);
            }
            for void in &lev.voids {
                subtract_rect(&mut m, *void);
            }
            for ramp in &ramp_specs {
                if ramp.lower_level + 1 == level_idx as u32 {
                    for (row, col) in ramp.footprint_cells() {
                        m[row as usize][col as usize] = false;
                    }
                }
            }
            m
        })
        .collect();

    // Per-level grids carrying floor flags + explicit wall flags.
    let mut level_grids: Vec<Vec<Vec<GridCell>>> = b
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, lev)| {
            let mut grid = vec![vec![GridCell::default(); cols as usize]; rows as usize];
            mark_has_floor(&mut grid, &masks[level_idx]);
            for wall in &lev.walls {
                set_wall_segment(&mut grid, wall, cols, rows);
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

    // Generate walls per level.
    let mut all_walls: Vec<Wall> = Vec::new();
    for (level_idx, grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let mut tier = walls::generate_walls(grid, cols, rows, level_u8);
        tier = walls::merge_walls(tier);
        all_walls.extend(tier);
    }

    // Emit floors per mask.
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

fn ramp_spec_from_blueprint(r: &RampBlueprint) -> ramps::RampSpec {
    let [col0, row0, col_end, row_end] = r.rect;
    let along_x = matches!(r.up.as_str(), "east" | "west");
    let high_at_end = matches!(r.up.as_str(), "south" | "east");
    ramps::RampSpec {
        lower_level: r.lower_level,
        along_x,
        high_at_end,
        col0,
        row0,
        col_end,
        row_end,
    }
}

fn empty_mask(grid_cols: i32, grid_rows: i32) -> Mask {
    vec![vec![false; grid_cols as usize]; grid_rows as usize]
}

fn add_rect(mask: &mut Mask, rect: [i32; 4]) {
    let rows = mask.len() as i32;
    if rows == 0 {
        return;
    }
    let cols = mask[0].len() as i32;
    let [c0, r0, c_end, r_end] = rect;
    for row in r0.max(0)..r_end.min(rows) {
        for col in c0.max(0)..c_end.min(cols) {
            mask[row as usize][col as usize] = true;
        }
    }
}

fn subtract_rect(mask: &mut Mask, rect: [i32; 4]) {
    let rows = mask.len() as i32;
    if rows == 0 {
        return;
    }
    let cols = mask[0].len() as i32;
    let [c0, r0, c_end, r_end] = rect;
    for row in r0.max(0)..r_end.min(rows) {
        for col in c0.max(0)..c_end.min(cols) {
            mask[row as usize][col as usize] = false;
        }
    }
}

fn set_wall_segment(grid: &mut [Vec<GridCell>], wall: &WallBlueprint, grid_cols: i32, grid_rows: i32) {
    let [c0, r0] = wall.from;
    let [c1, r1] = wall.to;
    if r0 == r1 {
        for col in c0.min(c1)..c0.max(c1) {
            set_horizontal_wall(grid, r0, col, grid_rows);
        }
    } else {
        for row in r0.min(r1)..r0.max(r1) {
            set_vertical_wall(grid, row, c0, grid_cols);
        }
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
