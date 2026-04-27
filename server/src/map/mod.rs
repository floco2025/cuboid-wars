mod floors;
mod grid;
mod helpers;
mod lights;
mod mask;
mod ramps;
mod walls;

use crate::{
    constants::{ATRIUM_CELLS, BUILDING_FOOTPRINT_CELLS, FLOOR_OVERLAP, NUM_LEVELS, ROOFTOP_FOOTPRINT_CELLS},
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Floor, MapLayout, Wall},
};
use lights::generate_wall_lights;
use mask::{Mask, mark_has_floor, mark_has_floor_above};
use ramps::RampSpec;

pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Deterministic building-shaped map generation. Every map has the same
// 5-level skeleton: basement, lobby, rooms-low, rooms-high, rooftop. A main
// stair zig-zags through the lobby and rooms floors; a utility stair has two
// short legs (basement-to-lobby and rooms-high-to-rooftop) so the basement
// and rooftop are gated by single chokepoints.
//
// Step 2 of the rewrite: hardcoded shaft positions, no atrium, no rooftop
// setback. Subsequent steps add atrium, setback, and per-generation random
// variation.
#[must_use]
pub fn generate_grid() -> (MapLayout, GridConfig) {
    let grid_cols = GRID_COLS;
    let grid_rows = GRID_ROWS;

    // Footprint rectangles (inclusive col0/row0, exclusive col_end/row_end).
    let footprint = centered_rect(BUILDING_FOOTPRINT_CELLS, grid_cols, grid_rows);
    let rooftop = centered_rect(ROOFTOP_FOOTPRINT_CELLS, grid_cols, grid_rows);

    // Two real U-shaped staircases (West and East), each spanning the
    // entire building from basement through rooftop. Within each shaft the
    // ramps zig-zag between two columns and alternate direction so the
    // player walks up one flight, turns 180°, walks up the next. The shaft
    // footprint is the same on every floor it serves, including the rooftop.
    let mut ramp_specs = Vec::new();
    ramp_specs.extend(u_staircase_ramps(/*west_col*/ 2)); // West staircase, cols 2-3
    ramp_specs.extend(u_staircase_ramps(/*west_col*/ 16)); // East staircase, cols 16-17

    // Central atrium: a void above the lobby. The lobby floor itself is
    // solid; the void cuts through both rooms floors (levels 2 and 3) for
    // a fixed 2-storey atrium.
    let atrium = centered_rect(ATRIUM_CELLS, grid_cols, grid_rows);
    let atrium_top: u32 = 3;

    // Per-level masks: start from the level's footprint, then subtract
    // *only* the cells at the upper level above each ramp (the "ramp body"
    // — those cells don't have a flat floor because the ramp's wedge
    // occupies the volume up to the upper-level Y). The lower level's
    // floor stays intact under each ramp; the ramp is a wedge sitting on
    // that floor, not a replacement.
    let masks: Vec<Mask> = (0..NUM_LEVELS)
        .map(|level| {
            let base = if level == NUM_LEVELS - 1 { rooftop } else { footprint };
            let mut m = mask_from_rect(base, grid_cols, grid_rows);
            for r in &ramp_specs {
                if r.lower_level + 1 == level {
                    for (row, col) in r.footprint_cells() {
                        m[row as usize][col as usize] = false;
                    }
                }
            }
            if level >= 2 && level <= atrium_top {
                subtract_rect(&mut m, atrium);
            }
            m
        })
        .collect();

    let all_ramps = ramps::specs_to_ramps(&ramp_specs);

    // Level-0 grid for wall-lights and spawn-cell selection.
    let mut level0_grid = vec![vec![GridCell::default(); grid_cols as usize]; grid_rows as usize];
    mark_has_floor(&mut level0_grid, &masks[0]);
    ramps::apply_to_level0_grid(&mut level0_grid, &ramp_specs);
    if NUM_LEVELS > 1 {
        mark_has_floor_above(&mut level0_grid, &masks[1]);
    }

    let wall_lights = generate_wall_lights(&level0_grid);

    // Emit floors per finalized mask.
    let mut all_floors: Vec<Floor> = Vec::new();
    for (level, m) in masks.iter().enumerate() {
        let level_u8 = u8::try_from(level).unwrap_or(u8::MAX);
        let y = f32::from(level_u8) * LEVEL_HEIGHT;
        let mut tier = floors::emit_floor_tier(m, grid_cols, grid_rows, level_u8, y);
        if !FLOOR_OVERLAP {
            tier = floors::merge_floors(tier);
        }
        all_floors.extend(tier);
    }

    let map_layout = MapLayout {
        walls: Vec::<Wall>::new(),
        ramps: all_ramps,
        wall_lights,
        floors: all_floors,
    };

    (map_layout, GridConfig { grid: level0_grid })
}

// ============================================================================
// Helpers (will move to building.rs once Steps 3/4 grow them)
// ============================================================================

#[derive(Copy, Clone, Debug)]
struct Rect {
    col0: i32,
    row0: i32,
    col_end: i32,
    row_end: i32,
}

fn centered_rect(side: i32, grid_cols: i32, grid_rows: i32) -> Rect {
    let col0 = (grid_cols - side) / 2;
    let row0 = (grid_rows - side) / 2;
    Rect {
        col0,
        row0,
        col_end: col0 + side,
        row_end: row0 + side,
    }
}

fn mask_from_rect(rect: Rect, grid_cols: i32, grid_rows: i32) -> Mask {
    let mut m = vec![vec![false; grid_cols as usize]; grid_rows as usize];
    for row in rect.row0.max(0)..rect.row_end.min(grid_rows) {
        for col in rect.col0.max(0)..rect.col_end.min(grid_cols) {
            m[row as usize][col as usize] = true;
        }
    }
    m
}

fn subtract_rect(mask: &mut Mask, rect: Rect) {
    let rows = mask.len() as i32;
    if rows == 0 {
        return;
    }
    let cols = mask[0].len() as i32;
    for row in rect.row0.max(0)..rect.row_end.min(rows) {
        for col in rect.col0.max(0)..rect.col_end.min(cols) {
            mask[row as usize][col as usize] = false;
        }
    }
}

// 2-cell along-Z ramp at (col, row0). Footprint occupies (row0, col) and
// (row0 + 1, col). The base is at row0 (the north end); the high cell is at
// row0 + 1 (the south end), so the player enters from the north and walks
// south up the ramp. Exit cell is at (row0 + 2, col) on the upper level.
fn south_up_ramp(col: i32, row0: i32, lower_level: u32) -> RampSpec {
    RampSpec {
        lower_level,
        along_x: false,
        high_at_end: true,
        col0: col,
        row0,
        col_end: col + 1,
        row_end: row0 + 2,
    }
}

// Mirror of `south_up_ramp` — base at row0+1 (south), high at row0 (north),
// so the player enters from the south and walks north up the ramp.
fn north_up_ramp(col: i32, row0: i32, lower_level: u32) -> RampSpec {
    RampSpec {
        lower_level,
        along_x: false,
        high_at_end: false,
        col0: col,
        row0,
        col_end: col + 1,
        row_end: row0 + 2,
    }
}

// A U-shaped 4-flight staircase occupying a 2×4 shaft at (cols [west_col,
// west_col+1], rows 3..6). Same footprint on every floor it serves
// (basement → rooftop). The four ramps alternate columns and directions
// so the player zig-zags up:
//
//   - basement → lobby:        south-up at west_col
//   - lobby → rooms-low:       north-up at west_col+1
//   - rooms-low → rooms-high:  south-up at west_col
//   - rooms-high → rooftop:    north-up at west_col+1
//
// Exits and entries chain inside the shaft footprint — the player never
// has to leave the shaft between floors.
fn u_staircase_ramps(west_col: i32) -> Vec<RampSpec> {
    let east_col = west_col + 1;
    vec![
        south_up_ramp(west_col, 4, 0),
        north_up_ramp(east_col, 4, 1),
        south_up_ramp(west_col, 4, 2),
        north_up_ramp(east_col, 4, 3),
    ]
}
