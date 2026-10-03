use super::*;
use crate::test_geometry::{CELL, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS, geometry};

const WALL_HALF_THICKNESS: f32 = WALL_THICKNESS / 2.0;

fn empty_mask(cols: usize, rows: usize) -> Mask {
    vec![vec![false; cols]; rows]
}

fn trim(edges: &EdgeGrid, mask: &Mask, cols: i32, rows: i32) -> Vec<Floor> {
    emit_wall_trim(edges, mask, &geometry(cols, rows), 1, LEVEL_HEIGHT, CarrierId::WORLD)
}

fn one_horizontal_edge() -> EdgeGrid {
    let mut edges = EdgeGrid::new(1, 1);
    edges.horizontal[1][0] = true;
    edges
}

#[test]
fn a_horizontal_wall_gets_a_strip_under_it() {
    let mut edges = EdgeGrid::new(1, 1);
    edges.horizontal[1][0] = true;
    let floors = trim(&edges, &empty_mask(1, 1), 1, 1);

    let geometry = geometry(1, 1);
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
fn a_vertical_wall_gets_a_strip_under_it() {
    let mut edges = EdgeGrid::new(1, 1);
    edges.vertical[0][1] = true;
    let floors = trim(&edges, &empty_mask(1, 1), 1, 1);

    let geometry = geometry(1, 1);
    let half_w = geometry.width() / 2.0;
    let half_d = geometry.depth() / 2.0;
    assert_eq!(floors.len(), 1);
    assert_eq!(floors[0].x1, -half_w + CELL - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].x2, -half_w + CELL + WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z1, -half_d - WALL_HALF_THICKNESS);
    assert_eq!(floors[0].z2, -half_d + CELL + WALL_HALF_THICKNESS);
}

// Only this level's walls are capped: the level below having a wall there is neither here nor there.
#[test]
fn a_level_without_walls_gets_no_strips() {
    assert!(trim(&EdgeGrid::new(1, 1), &empty_mask(1, 1), 1, 1).is_empty());
}

#[test]
fn band_edges_are_the_upper_walls_and_the_barriers_over_a_wall() {
    let none = EdgeGrid::new(1, 1);
    let edge = one_horizontal_edge();
    assert!(!band_edges(&edge, &none, &none).horizontal[1][0]);
    assert!(band_edges(&none, &edge, &none).horizontal[1][0]);
    assert!(band_edges(&edge, &none, &edge).horizontal[1][0]);
    assert!(!band_edges(&none, &none, &edge).horizontal[1][0]);
}

#[test]
fn a_wall_beside_a_floor_slab_gets_no_strip() {
    let mut edges = EdgeGrid::new(1, 1);
    edges.horizontal[1][0] = true;
    edges.vertical[0][1] = true;
    assert!(trim(&edges, &vec![vec![true]], 1, 1).is_empty());
}

// The strip follows the wall's own segment, inset where a wall crosses its end.
#[test]
fn a_strip_takes_its_walls_endpoint_rules() {
    let mut edges = EdgeGrid::new(1, 2);
    edges.horizontal[1][0] = true;
    edges.vertical[0][0] = true;
    edges.vertical[1][0] = true;
    let floors = trim(&edges, &empty_mask(1, 2), 1, 2);

    let geometry = geometry(1, 2);
    let half_w = geometry.width() / 2.0;
    let across: Vec<_> = floors
        .iter()
        .filter(|floor| floor.x2 - floor.x1 > WALL_THICKNESS)
        .collect();
    assert_eq!((floors.len(), across.len()), (3, 1));
    assert_eq!(across[0].x1, -half_w + WALL_HALF_THICKNESS);
    assert_eq!(across[0].x2, -half_w + CELL + WALL_HALF_THICKNESS);
}
