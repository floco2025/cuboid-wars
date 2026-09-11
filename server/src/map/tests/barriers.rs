use super::*;
use crate::test_geometry::{BARRIER_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, geometry};
use common::protocol::BarrierKindId;

const RED: BarrierKindId = BarrierKindId(0);
const BLUE: BarrierKindId = BarrierKindId(1);
const GREEN: BarrierKindId = BarrierKindId(2);

fn h(x1: f32, x2: f32, z: f32, kind: BarrierKindId) -> Barrier {
    Barrier {
        id: Default::default(),

        switch: None,
        switch_inverted: false,

        x1,
        x2,
        z1: z,
        z2: z,
        level: 0,
        levels: 1,
        kind,
        y: 0.0,
        height: WALL_HEIGHT,
        width: BARRIER_THICKNESS,
        carrier: CarrierId::WORLD,
    }
}

fn v(x: f32, z1: f32, z2: f32, kind: BarrierKindId) -> Barrier {
    Barrier {
        id: Default::default(),

        switch: None,
        switch_inverted: false,

        x1: x,
        x2: x,
        z1,
        z2,
        level: 0,
        levels: 1,
        kind,
        y: 0.0,
        height: WALL_HEIGHT,
        width: BARRIER_THICKNESS,
        carrier: CarrierId::WORLD,
    }
}

fn edge(c0: i32, r0: i32, c1: i32, r1: i32, kind: BarrierKindId) -> BarrierEdge {
    BarrierEdge {
        switch: None,
        switch_inverted: false,

        edge: [c0, r0, c1, r1],
        kind,
    }
}

fn empty_mask() -> Mask {
    vec![vec![false; 2]; 2]
}

fn mask_with_floor(col: i32, row: i32) -> Mask {
    let mut mask = empty_mask();
    mask[row as usize][col as usize] = true;
    mask
}

#[test]
fn stacked_same_kind_barriers_with_no_floor_beside_become_one_record() {
    let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(1, 1, 0, 1, RED)]];
    let masks = vec![empty_mask(), empty_mask()];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 1);
    assert_eq!(barriers[0].level, 0);
    assert_eq!(barriers[0].levels, 2);
    assert_eq!(barriers[0].y, 0.0);
    assert_eq!(barriers[0].height, LEVEL_HEIGHT + WALL_HEIGHT);
}

#[test]
fn a_floor_slab_beside_the_edge_keeps_the_storeys_apart() {
    let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(0, 1, 1, 1, RED)]];
    let masks = vec![empty_mask(), mask_with_floor(0, 1)];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 2);
    assert!(barriers.iter().all(|b| b.levels == 1 && b.height == WALL_HEIGHT));
    assert_eq!(barriers[1].level, 1);
    assert_eq!(barriers[1].y, LEVEL_HEIGHT);
}

#[test]
fn a_floor_on_either_side_of_the_edge_splits_the_stack() {
    let levels = vec![vec![edge(1, 0, 1, 1, RED)], vec![edge(1, 0, 1, 1, RED)]];
    let masks = vec![empty_mask(), mask_with_floor(0, 0)];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 2);
}

#[test]
fn stacks_do_not_cross_kinds() {
    let levels = vec![vec![edge(0, 1, 1, 1, RED)], vec![edge(0, 1, 1, 1, BLUE)]];
    let masks = vec![empty_mask(), empty_mask()];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 2);
}

#[test]
fn a_run_spans_every_floorless_storey_and_restarts_past_a_floor() {
    let levels = vec![
        vec![edge(0, 1, 1, 1, RED)],
        vec![edge(0, 1, 1, 1, RED)],
        vec![edge(0, 1, 1, 1, RED)],
        vec![edge(0, 1, 1, 1, RED)],
    ];
    let masks = vec![empty_mask(), empty_mask(), empty_mask(), mask_with_floor(0, 0)];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 2);
    assert_eq!((barriers[0].level, barriers[0].levels), (0, 3));
    assert_eq!(barriers[0].height, 2.0 * LEVEL_HEIGHT + WALL_HEIGHT);
    assert_eq!((barriers[1].level, barriers[1].levels), (3, 1));
}

#[test]
fn a_missing_storey_ends_the_run() {
    let levels = vec![vec![edge(0, 1, 1, 1, RED)], Vec::new(), vec![edge(0, 1, 1, 1, RED)]];
    let masks = vec![empty_mask(), empty_mask(), empty_mask()];

    let barriers = stack_barriers(&levels, &masks, &geometry(2, 2), CarrierId::WORLD);

    assert_eq!(barriers.len(), 2);
    assert!(barriers.iter().all(|b| b.levels == 1));
}

#[test]
fn merges_adjacent_same_kind_horizontals() {
    let merged = merge_barriers(vec![
        h(0.0, 1.0, 0.0, RED),
        h(1.0, 2.0, 0.0, RED),
        h(2.0, 3.0, 0.0, RED),
    ]);
    assert_eq!(merged.len(), 1);
    assert!((merged[0].x1 - 0.0).abs() < MERGE_EPS);
    assert!((merged[0].x2 - 3.0).abs() < MERGE_EPS);
}

#[test]
fn does_not_merge_across_kind_change() {
    let merged = merge_barriers(vec![
        h(0.0, 1.0, 0.0, RED),
        h(1.0, 2.0, 0.0, RED),
        h(2.0, 3.0, 0.0, BLUE),
        h(3.0, 4.0, 0.0, RED),
    ]);
    assert_eq!(merged.len(), 3);
}

#[test]
fn does_not_merge_across_row_change() {
    let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), h(0.0, 1.0, 1.0, RED)]);
    assert_eq!(merged.len(), 2);
}

#[test]
fn does_not_merge_with_gap() {
    let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), h(2.0, 3.0, 0.0, RED)]);
    assert_eq!(merged.len(), 2);
}

#[test]
fn merges_verticals() {
    let merged = merge_barriers(vec![v(0.0, 0.0, 1.0, GREEN), v(0.0, 1.0, 2.0, GREEN)]);
    assert_eq!(merged.len(), 1);
    assert!((merged[0].z1 - 0.0).abs() < MERGE_EPS);
    assert!((merged[0].z2 - 2.0).abs() < MERGE_EPS);
}

#[test]
fn does_not_merge_across_axis() {
    let merged = merge_barriers(vec![h(0.0, 1.0, 0.0, RED), v(0.0, 0.0, 1.0, RED)]);
    assert_eq!(merged.len(), 2);
}

#[test]
fn does_not_merge_across_level() {
    let mut b0 = h(0.0, 1.0, 0.0, RED);
    let mut b1 = h(1.0, 2.0, 0.0, RED);
    b0.level = 0;
    b1.level = 1;
    let merged = merge_barriers(vec![b0, b1]);
    assert_eq!(merged.len(), 2);
}

#[test]
fn does_not_merge_across_storey_span() {
    let mut tall = h(0.0, 1.0, 0.0, RED);
    tall.levels = 2;
    tall.height = LEVEL_HEIGHT + WALL_HEIGHT;
    let merged = merge_barriers(vec![tall, h(1.0, 2.0, 0.0, RED)]);
    assert_eq!(merged.len(), 2);
}
