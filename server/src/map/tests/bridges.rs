use super::*;

const SKY: BridgeKindId = BridgeKindId(0);
const VOID: BridgeKindId = BridgeKindId(1);

fn rect(c0: i32, r0: i32, c1: i32, r1: i32, kind: BridgeKindId) -> BridgeRect {
    BridgeRect { c0, r0, c1, r1, kind }
}

fn cells(kind: BridgeKindId, cells: &[(i32, i32)]) -> Vec<(i32, i32, BridgeKindId)> {
    cells.iter().map(|&(col, row)| (col, row, kind)).collect()
}

#[test]
fn a_solid_block_becomes_one_rectangle() {
    let cells: Vec<_> = (0..3).flat_map(|row| (0..4).map(move |col| (col, row, SKY))).collect();
    assert_eq!(merge_light_bridges(&cells), [rect(0, 0, 4, 3, SKY)]);
}

#[test]
fn a_row_run_merges_and_a_gap_splits_it() {
    let cells = [(0, 0, SKY), (1, 0, SKY), (3, 0, SKY)];
    assert_eq!(
        merge_light_bridges(&cells),
        [rect(0, 0, 2, 1, SKY), rect(3, 0, 4, 1, SKY)]
    );
}

// A two-wide north-south walkway with one spur keeps a single slab along
// its length; the spur is the only extra collider.
#[test]
fn a_walkway_with_a_spur_is_not_split_across_the_walking_direction() {
    let cells = cells(SKY, &[(0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2)]);
    assert_eq!(
        merge_light_bridges(&cells),
        [rect(0, 0, 2, 3, SKY), rect(2, 1, 3, 2, SKY)]
    );
}

#[test]
fn an_east_west_walkway_with_spurs_on_both_sides_stays_one_slab() {
    let mut walkway: Vec<(i32, i32)> = (0..10).flat_map(|col| [(col, 1), (col, 2)]).collect();
    walkway.extend([(2, 0), (7, 3)]);
    let merged = merge_light_bridges(&cells(SKY, &walkway));
    assert_eq!(merged[0], rect(0, 1, 10, 3, SKY));
    assert_eq!(merged.len(), 3);
}

#[test]
fn an_l_shape_becomes_two_rectangles() {
    let cells = cells(SKY, &[(0, 0), (1, 0), (2, 0), (0, 1), (0, 2)]);
    let merged = merge_light_bridges(&cells);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged.iter().map(BridgeRect::area).sum::<i32>(), 5);
}

#[test]
fn kinds_never_merge_into_one_rectangle() {
    let cells = [(0, 0, SKY), (1, 0, VOID)];
    assert_eq!(
        merge_light_bridges(&cells),
        [rect(0, 0, 1, 1, SKY), rect(1, 0, 2, 1, VOID)]
    );
}

#[test]
fn duplicate_cells_collapse() {
    let cells = [(2, 2, SKY), (2, 2, SKY)];
    assert_eq!(merge_light_bridges(&cells), [rect(2, 2, 3, 3, SKY)]);
}
