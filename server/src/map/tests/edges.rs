use super::*;

#[test]
fn cell_side_lookup_sees_an_internal_edge_from_both_cells() {
    let mut edges = EdgeGrid::new(1, 2);
    edges.horizontal[1][0] = true;
    assert!(has_edge_on_cell_side(&edges, 0, 0, CellSide::South));
    assert!(has_edge_on_cell_side(&edges, 1, 0, CellSide::North));

    let mut edges = EdgeGrid::new(2, 1);
    edges.vertical[0][1] = true;
    assert!(has_edge_on_cell_side(&edges, 0, 0, CellSide::East));
    assert!(has_edge_on_cell_side(&edges, 0, 1, CellSide::West));
}
