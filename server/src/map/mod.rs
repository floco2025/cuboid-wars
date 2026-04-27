mod floors;
mod grid;
mod helpers;
mod lights;
mod mask;
mod ramps;
mod walls;

use rand::{RngExt, rng};

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
    let mut rng = rng();
    let grid_cols = GRID_COLS;
    let grid_rows = GRID_ROWS;

    // Footprint rectangles (inclusive col0/row0, exclusive col_end/row_end).
    let footprint = centered_rect(BUILDING_FOOTPRINT_CELLS, grid_cols, grid_rows);
    let rooftop = centered_rect(ROOFTOP_FOOTPRINT_CELLS, grid_cols, grid_rows);

    // Ramps. Each is a single 2-cell along-Z ramp; they're placed at the
    // building's perimeter, well clear of the central atrium, so neither
    // their footprints nor their entry/exit cells fall inside the atrium void.
    //
    // Main stair: a straight shot up the west wall, columns chained — main_lower
    // exits onto the row that main_upper enters from, so the player walks
    // straight from one ramp to the next without crossing the atrium.
    let main_lower = south_up_ramp(/*col*/ 2, /*row0*/ 4, /*lower*/ 1); // lobby -> rooms-low
    let main_upper = south_up_ramp(/*col*/ 2, /*row0*/ 7, /*lower*/ 2); // rooms-low -> rooms-high
    // Utility legs: separate XZs, both inside the rooftop setback for the
    // upper leg's exit to land on rooftop floor.
    let utility_low = south_up_ramp(/*col*/ 17, /*row0*/ 4, /*lower*/ 0); // basement -> lobby
    let utility_high = south_up_ramp(/*col*/ 14, /*row0*/ 4, /*lower*/ 3); // rooms-high -> rooftop
    let ramp_specs = vec![utility_low, main_lower, main_upper, utility_high];

    // Central atrium: a void above the lobby. The lobby floor itself is
    // solid (you stand on it and look up); the void cuts through rooms-low
    // (level 2) always, and rooms-high (level 3) with 50% probability for
    // either a 1- or 2-storey atrium.
    let atrium = centered_rect(ATRIUM_CELLS, grid_cols, grid_rows);
    let atrium_top: u32 = if rng.random_bool(0.5) { 2 } else { 3 };

    // Per-level masks: start from the level's footprint, subtract ramp
    // footprints (this level), subtract ramp body cells (level above a
    // ramp), subtract atrium void where applicable.
    let masks: Vec<Mask> = (0..NUM_LEVELS)
        .map(|level| {
            let base = if level == NUM_LEVELS - 1 { rooftop } else { footprint };
            let mut m = mask_from_rect(base, grid_cols, grid_rows);
            for r in &ramp_specs {
                if r.lower_level == level {
                    for (row, col) in r.footprint_cells() {
                        m[row as usize][col as usize] = false;
                    }
                }
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
