use super::*;

#[test]
fn cell_side_lookup_sees_internal_horizontal_edge_from_both_cells() {
    let mut edges = EdgeGrid::new(1, 2);
    edges.horizontal[1][0] = true;

    assert!(has_edge_on_cell_side(&edges, 0, 0, CellSide::South));
    assert!(has_edge_on_cell_side(&edges, 1, 0, CellSide::North));
}

#[test]
fn cell_side_lookup_sees_internal_vertical_edge_from_both_cells() {
    let mut edges = EdgeGrid::new(2, 1);
    edges.vertical[0][1] = true;

    assert!(has_edge_on_cell_side(&edges, 0, 0, CellSide::East));
    assert!(has_edge_on_cell_side(&edges, 0, 1, CellSide::West));
}

#[test]
fn grid_line_lookup_reads_boundary_edges() {
    let mut edges = EdgeGrid::new(1, 1);
    edges.horizontal[1][0] = true;
    edges.vertical[0][1] = true;

    assert!(has_horizontal_edge(&edges, 1, 0));
    assert!(has_vertical_edge(&edges, 0, 1));
}
