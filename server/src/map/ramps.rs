use crate::resources::CellGrid;
use common::{constants::*, protocol::Ramp};

// Internal representation of a placed ramp. Downstream code converts it to
// `Ramp` for the wire protocol and applies its flags to the matching lower-level
// grid.
#[derive(Debug, Clone)]
pub struct RampSpec {
    pub lower_level: u32,
    pub low: [i32; 2],
    pub high: [i32; 2],
}

impl RampSpec {
    pub(super) fn footprint_cells(&self) -> Vec<(i32, i32)> {
        let [col0, row0, col_end, row_end] = self.rect();
        let mut cells = Vec::new();
        for col in col0..col_end {
            for row in row0..row_end {
                cells.push((row, col));
            }
        }
        cells
    }

    fn rect(&self) -> [i32; 4] {
        [
            self.low[0].min(self.high[0]),
            self.low[1].min(self.high[1]),
            self.low[0].max(self.high[0]),
            self.low[1].max(self.high[1]),
        ]
    }

    fn to_ramp(&self) -> Ramp {
        let y_low = self.lower_level as f32 * LEVEL_HEIGHT;
        let y_high = (self.lower_level + 1) as f32 * LEVEL_HEIGHT;

        let x1 = (self.low[0] as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0));
        let z1 = (self.low[1] as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0));
        let x2 = (self.high[0] as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0));
        let z2 = (self.high[1] as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0));

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

// Apply ramp flags to a level's cell grid.
pub fn apply_to_level_cells(cells: &mut CellGrid, ramps: &[RampSpec], level: u32) {
    for ramp in ramps {
        if ramp.lower_level + 1 == level {
            for (row, col) in ramp.footprint_cells() {
                cells.rows[row as usize][col as usize].has_ramp_from_below = true;
            }
        }
        if ramp.lower_level != level {
            continue;
        }
        let [col0, row0, col_end, row_end] = ramp.rect();
        for (row, col) in ramp.footprint_cells() {
            cells.rows[row as usize][col as usize].has_ramp = true;
        }
        let width = (ramp.high[0] - ramp.low[0]).abs();
        let height = (ramp.high[1] - ramp.low[1]).abs();
        if width > height {
            for row in row0..row_end {
                if ramp.high[0] > ramp.low[0] {
                    cells.rows[row as usize][col0 as usize].ramp_base_west = true;
                    cells.rows[row as usize][(col_end - 1) as usize].ramp_top_east = true;
                } else {
                    cells.rows[row as usize][(col_end - 1) as usize].ramp_base_east = true;
                    cells.rows[row as usize][col0 as usize].ramp_top_west = true;
                }
            }
        } else {
            for col in col0..col_end {
                if ramp.high[1] > ramp.low[1] {
                    cells.rows[row0 as usize][col as usize].ramp_base_north = true;
                    cells.rows[(row_end - 1) as usize][col as usize].ramp_top_south = true;
                } else {
                    cells.rows[(row_end - 1) as usize][col as usize].ramp_base_south = true;
                    cells.rows[row0 as usize][col as usize].ramp_top_north = true;
                }
            }
        }
    }
}

pub fn specs_to_ramps(specs: &[RampSpec]) -> Vec<Ramp> {
    specs.iter().map(RampSpec::to_ramp).collect()
}
