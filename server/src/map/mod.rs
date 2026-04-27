mod floors;
mod grid;
mod helpers;
mod lights;
mod mask;
mod ramps;
mod walls;

use bevy::prelude::debug;
use rand::rng;

use crate::{
    constants::FLOOR_OVERLAP,
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Floor, MapLayout, Wall},
};
use lights::generate_wall_lights;
use mask::{Mask, generate_mask, mark_has_floor, mark_has_floor_above, target_count_for_level};

pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Generate the map. Phase 1 of the multi-level rebuild: every level's floor
// footprint is generated independently (mask growth from random seeds), ramps
// are placed greedily to maximize cross-level reachability, and walls are
// disabled. Phase 2 will re-enable walls.
//
// `GridConfig` carries a level-0 grid annotated with `has_floor_above` (for
// wall lights) and `has_ramp` / ramp-edge flags (for spawn-cell selection).
#[must_use]
pub fn generate_grid(num_levels: u32) -> (MapLayout, GridConfig) {
    let num_levels = num_levels.max(1);
    let mut rng = rng();

    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;

    // Independent per-level masks. Density lerps from FLOOR_GROUND_DENSITY to
    // FLOOR_TOP_DENSITY over the stack; intermediate levels interpolate. No
    // cross-level constraint, so atria, overhangs, and bridges all emerge
    // naturally.
    let mut masks: Vec<Mask> = Vec::with_capacity(num_levels as usize);
    for level in 0..num_levels {
        let target = target_count_for_level(level, num_levels, grid_cols, grid_rows);
        masks.push(generate_mask(&mut rng, target, grid_cols, grid_rows));
    }

    // Connectivity-driven ramps: keep adding the candidate that grows
    // reachability most until the world is connected or no candidate makes
    // progress.
    let ramp_specs = ramps::place_connecting_ramps(&mut rng, &masks, grid_cols, grid_rows);

    // Reserve the vertical space above each ramp footprint by clearing those
    // cells from the upper-level mask. Then drop any cells that the
    // clearing left orphaned.
    ramps::clear_footprint_cells_above_ramps(&mut masks, &ramp_specs);
    let dropped = ramps::prune_unreachable(&mut masks, &ramp_specs, grid_cols, grid_rows);
    if dropped.iter().any(|&n| n > 0) {
        debug!("pruned unreachable cells per level: {:?}", dropped);
    }

    let all_ramps = ramps::specs_to_ramps(&ramp_specs);

    // Level-0 grid for wall-lights and spawn-cell selection. Apply ramp flags
    // for ramps with lower_level == 0 and mark cells with floor at level 1.
    let mut level0_grid = vec![vec![GridCell::default(); grid_cols as usize]; grid_rows as usize];
    mark_has_floor(&mut level0_grid, &masks[0]);
    ramps::apply_to_level0_grid(&mut level0_grid, &ramp_specs);
    if num_levels > 1 {
        mark_has_floor_above(&mut level0_grid, &masks[1]);
    }

    let wall_lights = generate_wall_lights(&level0_grid);

    // Emit floors per finalized mask.
    let mut all_floors: Vec<Floor> = Vec::new();
    for (level, mask) in masks.iter().enumerate() {
        let level_u8 = u8::try_from(level).unwrap_or(u8::MAX);
        let y = f32::from(level_u8) * LEVEL_HEIGHT;
        let mut tier = floors::emit_floor_tier(mask, grid_cols, grid_rows, level_u8, y);
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
