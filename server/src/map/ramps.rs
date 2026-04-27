use crate::resources::GridCell;
use common::{constants::*, protocol::Ramp};

// Internal representation of a placed ramp. The deterministic building
// generator constructs these directly; downstream code converts them to
// `Ramp` for the wire protocol and applies their flags to the level-0 grid.
#[derive(Debug, Clone)]
pub struct RampSpec {
    pub lower_level: u32,
    pub along_x: bool,
    pub high_at_end: bool,
    pub col0: i32,
    pub row0: i32,
    pub col_end: i32,
    pub row_end: i32,
}

impl RampSpec {
    pub(super) fn footprint_cells(&self) -> Vec<(i32, i32)> {
        let mut cells = Vec::new();
        for col in self.col0..self.col_end {
            for row in self.row0..self.row_end {
                cells.push((row, col));
            }
        }
        cells
    }

    fn to_ramp(&self) -> Ramp {
        let y_low = self.lower_level as f32 * LEVEL_HEIGHT;
        let y_high = (self.lower_level + 1) as f32 * LEVEL_HEIGHT;

        let x_start = (self.col0 as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_start = (self.row0 as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        let x_end = (self.col_end as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_end = (self.row_end as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

        let (x1, z1, x2, z2) = if self.high_at_end {
            (x_start, z_start, x_end, z_end)
        } else {
            (x_end, z_end, x_start, z_start)
        };

        Ramp {
            x1,
            y1: y_low,
            z1,
            x2,
            y2: y_high,
            z2,
        }
    }
}

// Apply ramp flags to the level-0 grid for spawn-cell selection (player and
// item placement skip cells with `has_ramp`). Ramps at higher levels don't
// need flags here — the level-0 grid is the only one downstream consumers see.
pub fn apply_to_level0_grid(grid: &mut [Vec<GridCell>], ramps: &[RampSpec]) {
    for ramp in ramps {
        if ramp.lower_level != 0 {
            continue;
        }
        for (row, col) in ramp.footprint_cells() {
            grid[row as usize][col as usize].has_ramp = true;
        }
        if ramp.along_x {
            for row in ramp.row0..ramp.row_end {
                if ramp.high_at_end {
                    grid[row as usize][ramp.col0 as usize].ramp_base_west = true;
                    grid[row as usize][(ramp.col_end - 1) as usize].ramp_top_east = true;
                } else {
                    grid[row as usize][(ramp.col_end - 1) as usize].ramp_base_east = true;
                    grid[row as usize][ramp.col0 as usize].ramp_top_west = true;
                }
            }
        } else {
            for col in ramp.col0..ramp.col_end {
                if ramp.high_at_end {
                    grid[ramp.row0 as usize][col as usize].ramp_base_north = true;
                    grid[(ramp.row_end - 1) as usize][col as usize].ramp_top_south = true;
                } else {
                    grid[(ramp.row_end - 1) as usize][col as usize].ramp_base_south = true;
                    grid[ramp.row0 as usize][col as usize].ramp_top_north = true;
                }
            }
        }
    }
}

pub fn specs_to_ramps(specs: &[RampSpec]) -> Vec<Ramp> {
    specs.iter().map(RampSpec::to_ramp).collect()
}
