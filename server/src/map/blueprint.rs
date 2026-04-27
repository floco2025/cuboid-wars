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
    /// `[col0, row0, col_end, row_end]` — half-open. Cells inside this rect
    /// have floor unless excluded by `atrium` or a ramp body above them.
    pub floor_rect: [i32; 4],
    /// Optional `[col0, row0, col_end, row_end]` void inside `floor_rect`.
    #[serde(default)]
    pub atrium: Option<[i32; 4]>,
    /// Optional perimeter wall: walls run along this rect's boundary (with
    /// any specified doors). Inset from `floor_rect` to leave a recessed
    /// balcony around the building.
    #[serde(default)]
    pub perimeter: Option<RoomBlueprint>,
    /// Rectangles whose perimeter becomes interior walls. Cells inside the
    /// rect remain floor; only the boundary turns into walls (modulo doors).
    #[serde(default)]
    pub rooms: Vec<RoomBlueprint>,
}

#[derive(Debug, Deserialize)]
pub struct RoomBlueprint {
    /// `[col0, row0, col_end, row_end]` — the room's footprint.
    pub rect: [i32; 4],
    #[serde(default)]
    pub doors: Vec<DoorBlueprint>,
}

#[derive(Debug, Deserialize)]
pub struct DoorBlueprint {
    /// Which side of the room the door is on. "N", "S", "E", or "W".
    pub side: String,
    /// The col (for N/S sides) or row (for E/W sides) where the door cell sits.
    pub at: i32,
}

#[derive(Debug, Deserialize)]
pub struct RampBlueprint {
    /// "south-up" — base at row0 (north end), high at row0+1 (south end);
    /// player walks south up the ramp; exit cell is (row0+2, col).
    /// "north-up" — base at row0+1 (south end), high at row0 (north end);
    /// player walks north up; exit at (row0-1, col).
    pub direction: String,
    pub col: i32,
    pub row0: i32,
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
        validate_rect(&level.floor_rect, b.grid_cols, b.grid_rows)
            .with_context(|| format!("level {} ({}): floor_rect", idx, level.name))?;
        if let Some(atrium) = level.atrium {
            validate_rect(&atrium, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): atrium", idx, level.name))?;
            if !contains(&level.floor_rect, &atrium) {
                return Err(anyhow!(
                    "level {} ({}): atrium {:?} not inside floor_rect {:?}",
                    idx,
                    level.name,
                    atrium,
                    level.floor_rect
                ));
            }
        }
        if let Some(perim) = &level.perimeter {
            validate_rect(&perim.rect, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): perimeter.rect", idx, level.name))?;
            if !contains(&level.floor_rect, &perim.rect) {
                return Err(anyhow!(
                    "level {} ({}): perimeter {:?} not inside floor_rect {:?}",
                    idx,
                    level.name,
                    perim.rect,
                    level.floor_rect
                ));
            }
            for (door_idx, door) in perim.doors.iter().enumerate() {
                validate_door(door, &perim.rect)
                    .with_context(|| format!("level {} ({}): perimeter.doors[{}]", idx, level.name, door_idx))?;
            }
        }
        for (room_idx, room) in level.rooms.iter().enumerate() {
            validate_rect(&room.rect, b.grid_cols, b.grid_rows)
                .with_context(|| format!("level {} ({}): rooms[{}]", idx, level.name, room_idx))?;
            if !contains(&level.floor_rect, &room.rect) {
                return Err(anyhow!(
                    "level {} ({}): rooms[{}] {:?} not inside floor_rect {:?}",
                    idx,
                    level.name,
                    room_idx,
                    room.rect,
                    level.floor_rect
                ));
            }
            for (door_idx, door) in room.doors.iter().enumerate() {
                validate_door(door, &room.rect).with_context(|| {
                    format!(
                        "level {} ({}): rooms[{}].doors[{}]",
                        idx, level.name, room_idx, door_idx
                    )
                })?;
            }
        }
    }

    for (idx, ramp) in b.ramps.iter().enumerate() {
        match ramp.direction.as_str() {
            "south-up" | "north-up" => {}
            other => return Err(anyhow!("ramps[{}]: unknown direction {:?}", idx, other)),
        }
        if (ramp.lower_level + 1) as usize >= b.levels.len() && ramp.lower_level as usize + 1 != b.levels.len() {
            return Err(anyhow!(
                "ramps[{}]: lower_level {} has no upper level (num_levels = {})",
                idx,
                ramp.lower_level,
                b.num_levels
            ));
        }
        if (ramp.lower_level + 1) >= b.num_levels {
            return Err(anyhow!(
                "ramps[{}]: lower_level {} has no upper level (num_levels = {})",
                idx,
                ramp.lower_level,
                b.num_levels
            ));
        }
        // Footprint cells in bounds.
        if ramp.col < 0 || ramp.col >= b.grid_cols || ramp.row0 < 0 || ramp.row0 + 2 > b.grid_rows {
            return Err(anyhow!(
                "ramps[{}]: footprint at col={} row0={} out of grid {}x{}",
                idx,
                ramp.col,
                ramp.row0,
                b.grid_cols,
                b.grid_rows
            ));
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

fn contains(outer: &[i32; 4], inner: &[i32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
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

    // Per-level masks: floor_rect minus atrium minus ramp body cells.
    let masks: Vec<Mask> = b
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, lev)| {
            let mut m = mask_from_rect(lev.floor_rect, cols, rows);
            if let Some(atrium) = lev.atrium {
                subtract_rect(&mut m, atrium);
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

    // Per-level grids carrying floor flags + wall flags. Wall flags come
    // from each level's `rooms` (room boundary => wall, doors => gap).
    let mut level_grids: Vec<Vec<Vec<GridCell>>> = b
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, lev)| {
            let mut grid = vec![vec![GridCell::default(); cols as usize]; rows as usize];
            mark_has_floor(&mut grid, &masks[level_idx]);
            // First pass: set walls for the perimeter (if any) and every
            // room boundary.
            if let Some(perim) = &lev.perimeter {
                set_room_walls(&mut grid, perim.rect, cols, rows);
            }
            for room in &lev.rooms {
                set_room_walls(&mut grid, room.rect, cols, rows);
            }
            // Second pass: clear door cells (overrides walls so adjacent
            // rooms sharing a boundary don't fight over the door).
            if let Some(perim) = &lev.perimeter {
                for door in &perim.doors {
                    clear_door(&mut grid, perim.rect, door, cols, rows);
                }
            }
            for room in &lev.rooms {
                for door in &room.doors {
                    clear_door(&mut grid, room.rect, door, cols, rows);
                }
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
    let high_at_end = r.direction == "south-up";
    ramps::RampSpec {
        lower_level: r.lower_level,
        along_x: false,
        high_at_end,
        col0: r.col,
        row0: r.row0,
        col_end: r.col + 1,
        row_end: r.row0 + 2,
    }
}

fn mask_from_rect(rect: [i32; 4], grid_cols: i32, grid_rows: i32) -> Mask {
    let mut m = vec![vec![false; grid_cols as usize]; grid_rows as usize];
    let [c0, r0, c_end, r_end] = rect;
    for row in r0.max(0)..r_end.min(grid_rows) {
        for col in c0.max(0)..c_end.min(grid_cols) {
            m[row as usize][col as usize] = true;
        }
    }
    m
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

fn set_room_walls(grid: &mut [Vec<GridCell>], rect: [i32; 4], grid_cols: i32, grid_rows: i32) {
    let [c0, r0, c_end, r_end] = rect;
    // North wall: grid line row=r0, cols c0..c_end.
    for col in c0..c_end {
        set_horizontal_wall(grid, r0, col, grid_rows);
    }
    // South wall: grid line row=r_end.
    for col in c0..c_end {
        set_horizontal_wall(grid, r_end, col, grid_rows);
    }
    // West wall: grid line col=c0, rows r0..r_end.
    for row in r0..r_end {
        set_vertical_wall(grid, row, c0, grid_cols);
    }
    // East wall: grid line col=c_end.
    for row in r0..r_end {
        set_vertical_wall(grid, row, c_end, grid_cols);
    }
}

fn clear_door(grid: &mut [Vec<GridCell>], room_rect: [i32; 4], door: &DoorBlueprint, grid_cols: i32, grid_rows: i32) {
    let [c0, r0, c_end, r_end] = room_rect;
    match door.side.as_str() {
        "N" => clear_horizontal_wall(grid, r0, door.at, grid_rows),
        "S" => clear_horizontal_wall(grid, r_end, door.at, grid_rows),
        "W" => clear_vertical_wall(grid, door.at, c0, grid_cols),
        "E" => clear_vertical_wall(grid, door.at, c_end, grid_cols),
        _ => {}
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

fn clear_horizontal_wall(grid: &mut [Vec<GridCell>], row: i32, col: i32, grid_rows: i32) {
    if row == 0 {
        grid[0][col as usize].has_north_wall = false;
    } else if row == grid_rows {
        grid[(grid_rows - 1) as usize][col as usize].has_south_wall = false;
    } else {
        grid[row as usize][col as usize].has_north_wall = false;
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

fn clear_vertical_wall(grid: &mut [Vec<GridCell>], row: i32, col: i32, grid_cols: i32) {
    if col == 0 {
        grid[row as usize][0].has_west_wall = false;
    } else if col == grid_cols {
        grid[row as usize][(grid_cols - 1) as usize].has_east_wall = false;
    } else {
        grid[row as usize][col as usize].has_west_wall = false;
    }
}

fn validate_door(door: &DoorBlueprint, room_rect: &[i32; 4]) -> Result<()> {
    let [c0, r0, c_end, r_end] = *room_rect;
    match door.side.as_str() {
        "N" | "S" => {
            if door.at < c0 || door.at >= c_end {
                return Err(anyhow!(
                    "door at col {} on side {} not inside room cols [{}..{})",
                    door.at,
                    door.side,
                    c0,
                    c_end
                ));
            }
        }
        "E" | "W" => {
            if door.at < r0 || door.at >= r_end {
                return Err(anyhow!(
                    "door at row {} on side {} not inside room rows [{}..{})",
                    door.at,
                    door.side,
                    r0,
                    r_end
                ));
            }
        }
        other => return Err(anyhow!("unknown door side {:?} (expected N/S/E/W)", other)),
    }
    Ok(())
}
