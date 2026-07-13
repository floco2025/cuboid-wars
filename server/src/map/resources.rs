use bevy::prelude::Resource;

use common::map::MapGeometry;

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
    // Wall edges: block movement and generate the visible wall geometry.
    pub edges: EdgeGrid,
    // Barrier edges: block actor pathfinding only. Holds the barriers an actor
    // can never pass — those NOT controlled by a pressure plate (actors can't
    // carry keys). Pressure-plate barriers are omitted (treated as open): they
    // seal a room with no alternate route, so assuming open lets a returning
    // actor path home and physics holds it at the barrier until someone opens
    // it. No geometry here; barriers render from their own world-space list.
    pub barrier_edges: EdgeGrid,
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

// Map-authored item placement, compiled from the map's `items` list with
// key kinds already resolved against the `BarrierKindTable`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedItem {
    pub level: u8,
    pub col: i32,
    pub row: i32,
    pub item_type: common::protocol::ItemType,
}

// Server-side runtime form of a pressure plate. Keeps the original
// (col, row) grid coords so `player_on_plate` can compute the inner-25%
// rect each tick. The wire variant (`common::protocol::PressurePlate`)
// carries world coords for the client renderer; this one stays in grid
// space. Distinct names so grep / jump-to-def isn't ambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PressurePlateRuntime {
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
    pub placed_items: Vec<PlacedItem>,
    pub pressure_plates: Vec<PressurePlateRuntime>,
}
