use crate::map::CellGrid;
use common::{
    map::MapGeometry,
    protocol::{CarrierId, Ramp},
};

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

    // Horizontal wall edges at the ramp's high end (where the slope reaches
    // the upper level), in `EdgeGrid.horizontal` index space. Returned only
    // for z-axis ramps — x-axis ramps' high end is a *vertical* edge, and
    // the corner-filler logic in `emit_floor_tier` only fires on N/S
    // suppressed extensions, so vertical edges are not relevant.
    pub(super) fn high_end_horizontal_edges(&self) -> Vec<(i32, i32)> {
        let [col0, row0, col_end, row_end] = self.rect();
        let width = (self.high[0] - self.low[0]).abs();
        let height = (self.high[1] - self.low[1]).abs();
        if width > height {
            return Vec::new();
        }
        let edge_row = if self.high[1] > self.low[1] { row_end } else { row0 };
        (col0..col_end).map(|col| (edge_row, col)).collect()
    }

    fn rect(&self) -> [i32; 4] {
        [
            self.low[0].min(self.high[0]),
            self.low[1].min(self.high[1]),
            self.low[0].max(self.high[0]),
            self.low[1].max(self.high[1]),
        ]
    }

    fn to_ramp(&self, geometry: &MapGeometry) -> Ramp {
        let y_low = geometry.level_y(u8::try_from(self.lower_level).unwrap_or(u8::MAX));
        let y_high = y_low + geometry.level_height();

        Ramp {
            x1: geometry.cell_to_world_x(self.low[0]),
            y1: y_low,
            z1: geometry.cell_to_world_z(self.low[1]),
            x2: geometry.cell_to_world_x(self.high[0]),
            y2: y_high,
            z2: geometry.cell_to_world_z(self.high[1]),
            carrier: CarrierId::WORLD,
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

pub fn specs_to_ramps(geometry: &MapGeometry, specs: &[RampSpec]) -> Vec<Ramp> {
    specs.iter().map(|spec| spec.to_ramp(geometry)).collect()
}
