// A boolean grid covering the whole map; `mask[row][col] == true` means the
// level this mask belongs to has floor at that cell.
pub type Mask = Vec<Vec<bool>>;

use crate::resources::CellGrid;

// Mark `has_floor_above` on cells of `grid` where `upper_mask[r][c]` is set.
// Used by the wall-lights generator to skip cells that are under a roof.
pub fn mark_has_floor_above(cells: &mut CellGrid, upper_mask: &Mask) {
    for (row_idx, row) in cells.rows.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if upper_mask[row_idx][col_idx] {
                cell.has_floor_above = true;
            }
        }
    }
}

// Mark regular floor cells. Automatic spawn placement and wall-light placement
// only use this flag, not `has_floor_slab`.
pub fn mark_has_floor(cells: &mut CellGrid, mask: &Mask) {
    for (row_idx, row) in cells.rows.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if mask[row_idx][col_idx] {
                cell.has_floor = true;
            }
        }
    }
}

// Mark physical floor slabs. This includes regular floors and inaccessible
// floors, and is used for light-exposure blocking.
pub fn mark_has_floor_slab(cells: &mut CellGrid, mask: &Mask) {
    for (row_idx, row) in cells.rows.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if mask[row_idx][col_idx] {
                cell.has_floor_slab = true;
            }
        }
    }
}
