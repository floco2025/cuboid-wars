use super::*;
use crate::test_geometry::{CELL, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS, geometry};

const WALL_HALF_THICKNESS: f32 = WALL_THICKNESS / 2.0;

fn empty_mask(cols: usize, rows: usize) -> Mask {
    vec![vec![false; cols]; rows]
}

#[test]
fn stacked_horizontal_wall_emits_physical_trim_strip() {
    let mut lower_edges = EdgeGrid::new(1, 1);
    let mut upper_edges = EdgeGrid::new(1, 1);
    let upper_mask = empty_mask(1, 1);
    lower_edges.horizontal[1][0] = true;
    upper_edges.horizontal[1][0] = true;

    let geometry = geometry(1, 1);
    let floors = emit_stacked_wall_trim(
        &lower_edges,
        &upper_edges,
        &upper_mask,
        &geometry,
        1,
        LEVEL_HEIGHT,
        CarrierId::WORLD,
    );

    let half_w = geometry.width() / 2.0;
    let half_d = geometry.depth() / 2.0;
    assert_eq!(floors.len(), 1);
    assert_eq!(floors[0].x1, -half_w - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].x2, -half_w + CELL + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z1, -half_d + CELL - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z2, -half_d + CELL + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].y, LEVEL_HEIGHT);
    assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
    assert_eq!(floors[0].level, 1);
}

#[test]
fn stacked_vertical_wall_emits_physical_trim_strip() {
    let mut lower_edges = EdgeGrid::new(1, 1);
    let mut upper_edges = EdgeGrid::new(1, 1);
    let upper_mask = empty_mask(1, 1);
    lower_edges.vertical[0][1] = true;
    upper_edges.vertical[0][1] = true;

    let geometry = geometry(1, 1);
    let floors = emit_stacked_wall_trim(
        &lower_edges,
        &upper_edges,
        &upper_mask,
        &geometry,
        1,
        LEVEL_HEIGHT,
        CarrierId::WORLD,
    );

    let half_w = geometry.width() / 2.0;
    let half_d = geometry.depth() / 2.0;
    assert_eq!(floors.len(), 1);
    assert_eq!(floors[0].x1, -half_w + CELL - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].x2, -half_w + CELL + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z1, -half_d - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z2, -half_d + CELL + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].y, LEVEL_HEIGHT);
    assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
    assert_eq!(floors[0].level, 1);
}

#[test]
fn unstacked_wall_does_not_emit_trim_strip() {
    let mut lower_edges = EdgeGrid::new(1, 1);
    let upper_edges = EdgeGrid::new(1, 1);
    let upper_mask = empty_mask(1, 1);
    lower_edges.horizontal[1][0] = true;

    let geometry = geometry(1, 1);
    let floors = emit_stacked_wall_trim(
        &lower_edges,
        &upper_edges,
        &upper_mask,
        &geometry,
        1,
        LEVEL_HEIGHT,
        CarrierId::WORLD,
    );

    assert!(floors.is_empty());
}

#[test]
fn stacked_wall_next_to_upper_floor_does_not_duplicate_trim() {
    let mut lower_edges = EdgeGrid::new(1, 1);
    let mut upper_edges = EdgeGrid::new(1, 1);
    let upper_mask = vec![vec![true]];
    lower_edges.horizontal[1][0] = true;
    upper_edges.horizontal[1][0] = true;
    lower_edges.vertical[0][1] = true;
    upper_edges.vertical[0][1] = true;

    let geometry = geometry(1, 1);
    let floors = emit_stacked_wall_trim(
        &lower_edges,
        &upper_edges,
        &upper_mask,
        &geometry,
        1,
        LEVEL_HEIGHT,
        CarrierId::WORLD,
    );

    assert!(floors.is_empty());
}

#[test]
fn stacked_trim_uses_overlapping_wall_endpoint_rules() {
    let mut lower_edges = EdgeGrid::new(1, 2);
    let mut upper_edges = EdgeGrid::new(1, 2);
    let upper_mask = empty_mask(1, 2);
    lower_edges.horizontal[1][0] = true;
    upper_edges.horizontal[1][0] = true;
    upper_edges.vertical[0][0] = true;
    upper_edges.vertical[1][0] = true;

    let geometry = geometry(1, 2);
    let floors = emit_stacked_wall_trim(
        &lower_edges,
        &upper_edges,
        &upper_mask,
        &geometry,
        1,
        LEVEL_HEIGHT,
        CarrierId::WORLD,
    );

    let half_w = geometry.width() / 2.0;
    assert_eq!(floors.len(), 1);
    assert_eq!(floors[0].x1, -half_w + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].x2, -half_w + CELL + WALL_HALF_THICKNESS);
}
