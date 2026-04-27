// A boolean grid covering the whole map; `mask[row][col] == true` means the
// level this mask belongs to has floor at that cell.
pub type Mask = Vec<Vec<bool>>;

// Mark `has_floor_above` on cells of `grid` where `upper_mask[r][c]` is set.
// Used by the wall-lights generator to skip cells that are under a roof.
pub fn mark_has_floor_above(grid: &mut [Vec<crate::resources::GridCell>], upper_mask: &Mask) {
    for (row_idx, row) in grid.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if upper_mask[row_idx][col_idx] {
                cell.has_floor_above = true;
            }
        }
    }
}

// Mark `has_floor` on cells of `grid` where `mask[r][c]` is set. Used by the
// spawn placer to skip cells without a level-0 floor.
pub fn mark_has_floor(grid: &mut [Vec<crate::resources::GridCell>], mask: &Mask) {
    for (row_idx, row) in grid.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if mask[row_idx][col_idx] {
                cell.has_floor = true;
            }
        }
    }
}
