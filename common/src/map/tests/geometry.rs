use crate::test_geometry::{CELL, geometry};

#[test]
fn cell_center_round_trips_to_the_same_cell() {
    let geometry = geometry(30, 20);
    for col in [0, 1, 15, 29] {
        let center_x = geometry.cell_to_world_x(col) + CELL / 2.0;
        assert_eq!(geometry.cell_col_containing_x(center_x), col);
    }
    for row in [0, 7, 19] {
        let center_z = geometry.cell_to_world_z(row) + CELL / 2.0;
        assert_eq!(geometry.cell_row_containing_z(center_z), row);
    }
}

#[test]
fn containing_lookup_floors_while_nearest_lookup_rounds() {
    let geometry = geometry(10, 10);
    // Just past a grid line: still the same containing cell, but the
    // nearest grid line snaps back.
    let line_x = geometry.cell_to_world_x(4);
    assert_eq!(geometry.cell_col_containing_x(line_x + 0.1), 4);
    assert_eq!(geometry.nearest_grid_col_to_x(line_x + 0.1), 4);
    // Just before the next line: containing cell unchanged, nearest
    // line rounds up.
    let almost_next = line_x + CELL - 0.1;
    assert_eq!(geometry.cell_col_containing_x(almost_next), 4);
    assert_eq!(geometry.nearest_grid_col_to_x(almost_next), 5);
}
