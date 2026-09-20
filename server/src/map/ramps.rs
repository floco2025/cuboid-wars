use crate::map::{CellGrid, CellSide, EdgeGrid, LevelGrid, has_edge_on_cell_side};
use common::{
    map::MapGeometry,
    protocol::{CarrierId, FaceMaterials, Ramp, RampDirection, RampShape},
};

// Internal representation of a placed ramp. Downstream code converts it to
// `Ramp` for the wire protocol and applies its flags to the level grids it
// spans. `cols` and `rows` are grid lines, the end exclusive as a cell range.
#[derive(Debug, Clone)]
pub struct RampSpec {
    pub lower_level: u32,
    pub levels: u32,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub direction: RampDirection,
    pub shape: RampShape,
    pub materials: FaceMaterials,
}

impl RampSpec {
    pub(super) fn footprint_cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (self.rows[0]..self.rows[1]).flat_map(|row| (self.cols[0]..self.cols[1]).map(move |col| (row, col)))
    }

    // The footprint cells along one of its sides.
    fn edge_cells(&self, side: CellSide) -> Vec<(i32, i32)> {
        let [col0, col_end] = self.cols;
        let [row0, row_end] = self.rows;
        match side {
            CellSide::North => (col0..col_end).map(|col| (row0, col)).collect(),
            CellSide::South => (col0..col_end).map(|col| (row_end - 1, col)).collect(),
            CellSide::West => (row0..row_end).map(|row| (row, col0)).collect(),
            CellSide::East => (row0..row_end).map(|row| (row, col_end - 1)).collect(),
        }
    }

    // Marks the grid edges along one side of the footprint.
    pub(super) fn mark_edge(&self, side: RampDirection, edges: &mut EdgeGrid) {
        let rect = [self.cols[0], self.rows[0], self.cols[1], self.rows[1]];
        for (axis, row, col) in map_core::geometry::side_edges(rect, side) {
            if axis == 'h' {
                edges.horizontal[row as usize][col as usize] = true;
            } else {
                edges.vertical[row as usize][col as usize] = true;
            }
        }
    }

    // Whether a wall runs along this side of the footprint on a storey the
    // slope passes through.
    fn walled(&self, side: CellSide, levels: &[LevelGrid]) -> bool {
        let storeys = self.lower_level as usize..(self.lower_level + self.levels) as usize;
        levels.get(storeys).is_some_and(|grids| {
            grids.iter().any(|grid| {
                self.edge_cells(side)
                    .iter()
                    .any(|&(row, col)| has_edge_on_cell_side(&grid.edges, row, col, side))
            })
        })
    }

    // A plank is as wide as the walkway it continues: like a floor slab it
    // overhangs its cells by half a wall on each side of the run. Along a wall
    // it stays flush, since the slope would cut through the wall's far face.
    fn widen_plank(&self, ramp: &mut Ramp, levels: &[LevelGrid], pad: f32) {
        if self.shape != RampShape::Plank {
            return;
        }
        let across = match self.direction {
            RampDirection::North | RampDirection::South => [CellSide::West, CellSide::East],
            RampDirection::East | RampDirection::West => [CellSide::North, CellSide::South],
        };
        for side in across.into_iter().filter(|side| !self.walled(*side, levels)) {
            match side {
                CellSide::West => ramp.x1 -= pad,
                CellSide::East => ramp.x2 += pad,
                CellSide::North => ramp.z1 -= pad,
                CellSide::South => ramp.z2 += pad,
            }
        }
    }

    pub(super) fn to_ramp(&self, geometry: &MapGeometry, carrier: CarrierId) -> Ramp {
        let level = u8::try_from(self.lower_level).unwrap_or(u8::MAX);
        let levels = u8::try_from(self.levels).unwrap_or(u8::MAX);
        Ramp {
            x1: geometry.cell_to_world_x(self.cols[0]),
            z1: geometry.cell_to_world_z(self.rows[0]),
            x2: geometry.cell_to_world_x(self.cols[1]),
            z2: geometry.cell_to_world_z(self.rows[1]),
            y: geometry.level_y(level),
            height: f32::from(levels) * geometry.level_height(),
            direction: self.direction,
            shape: self.shape,
            thickness: geometry.floor_thickness(),
            level,
            levels,
            carrier,
        }
    }
}

// Apply ramp flags to a level's cell grid: the slope on the ramp's own level,
// and the way down to it on every storey it passes or arrives at.
pub fn apply_to_level_cells(cells: &mut CellGrid, ramps: &[RampSpec], level: u32) {
    for ramp in ramps {
        if ramp.lower_level < level && level <= ramp.lower_level + ramp.levels {
            let below = u8::try_from(level - ramp.lower_level).unwrap_or(u8::MAX);
            for (row, col) in ramp.footprint_cells() {
                cells.rows[row as usize][col as usize].ramp_below = below;
            }
        }
        if ramp.lower_level != level {
            continue;
        }
        for (row, col) in ramp.footprint_cells() {
            cells.rows[row as usize][col as usize].has_ramp = true;
        }
    }
}

pub fn specs_to_ramps(
    geometry: &MapGeometry,
    specs: &[RampSpec],
    levels: &[LevelGrid],
    carrier: CarrierId,
) -> Vec<Ramp> {
    specs
        .iter()
        .map(|spec| {
            let mut ramp = spec.to_ramp(geometry, carrier);
            spec.widen_plank(&mut ramp, levels, geometry.wall_half_thickness());
            ramp
        })
        .collect()
}
