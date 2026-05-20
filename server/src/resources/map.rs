use bevy::prelude::Resource;

use common::map_geometry::MapGeometry;

// Cell flags.
#[derive(Copy, Clone, Debug, Default)]
pub struct Cell {
    pub has_ramp: bool,
    pub has_ramp_from_below: bool,
    pub has_floor: bool,
    pub has_floor_slab: bool,
    pub has_floor_above: bool,
    pub ramp_base_north: bool,
    pub ramp_base_south: bool,
    pub ramp_base_west: bool,
    pub ramp_base_east: bool,
    pub ramp_top_north: bool,
    pub ramp_top_south: bool,
    pub ramp_top_west: bool,
    pub ramp_top_east: bool,
}

impl Cell {
    // Spawn zones may target any cell that is not an obstruction and not an
    // inaccessible-floor slab. Empty cells are fine; flying actors do not need
    // floor underfoot.
    #[must_use]
    pub fn is_spawnable(&self) -> bool {
        if self.has_ramp {
            return false;
        }
        if self.has_floor_slab && !self.has_floor {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug)]
pub struct CellGrid {
    pub rows: Vec<Vec<Cell>>,
}

impl CellGrid {
    #[must_use]
    pub fn new(grid_cols: i32, grid_rows: i32) -> Self {
        Self {
            rows: vec![vec![Cell::default(); grid_cols as usize]; grid_rows as usize],
        }
    }
}

// Edges live on grid lines, not in cells.
//
// horizontal[row][col] covers the horizontal segment from grid point
// (col, row) to (col + 1, row), so its dimensions are
// (grid_rows + 1) x grid_cols.
//
// vertical[row][col] covers the vertical segment from grid point
// (col, row) to (col, row + 1), so its dimensions are
// grid_rows x (grid_cols + 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGrid {
    pub horizontal: Vec<Vec<bool>>,
    pub vertical: Vec<Vec<bool>>,
}

impl EdgeGrid {
    #[must_use]
    pub fn new(grid_cols: i32, grid_rows: i32) -> Self {
        Self {
            horizontal: vec![vec![false; grid_cols as usize]; grid_rows as usize + 1],
            vertical: vec![vec![false; grid_cols as usize + 1]; grid_rows as usize],
        }
    }
}

#[derive(Clone, Debug)]
pub struct LevelGrid {
    pub cells: CellGrid,
    pub edges: EdgeGrid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpawnZone {
    pub level: u8,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub kind: String,
    pub count: u32,
}

impl ActorSpawnZone {
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }

    #[must_use]
    pub fn xz_bounds(&self, geometry: &MapGeometry) -> (f32, f32, f32, f32) {
        let min_x = geometry.cell_to_world_x(self.cols[0]);
        let max_x = geometry.cell_to_world_x(self.cols[1]);
        let min_z = geometry.cell_to_world_z(self.rows[0]);
        let max_z = geometry.cell_to_world_z(self.rows[1]);
        (min_x, min_z, max_x, max_z)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSpawnZone {
    pub level: u8,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
}

impl PlayerSpawnZone {
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieSpawnZone {
    pub level: u8,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
}

impl CookieSpawnZone {
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySpawnZone {
    pub level: u8,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub kind: common::protocol::BarrierKindId,
}

impl KeySpawnZone {
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }
}

// Runtime mirror of `common::protocol::PressurePlate`. Server-side; the wire
// version lives on `MapLayout` and ships in `SInit`. Identical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PressurePlate {
    pub level: u8,
    pub col: i32,
    pub row: i32,
    pub kind: common::protocol::BarrierKindId,
}

#[derive(Resource, Clone)]
pub struct MapConfig {
    pub levels: Vec<LevelGrid>,
    pub actor_spawn_zones: Vec<ActorSpawnZone>,
    pub player_spawn_zones: Vec<PlayerSpawnZone>,
    pub cookie_spawn_zones: Vec<CookieSpawnZone>,
    pub key_spawn_zones: Vec<KeySpawnZone>,
    pub pressure_plates: Vec<PressurePlate>,
}
