use super::*;
use crate::test_geometry::{WALL_HEIGHT, WALL_THICKNESS};

fn h_wall(x1: f32, x2: f32, z: f32) -> Wall {
    Wall {
        x1,
        x2,
        z1: z,
        z2: z,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

fn v_wall(x: f32, z1: f32, z2: f32) -> Wall {
    Wall {
        x1: x,
        x2: x,
        z1,
        z2,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

// An L of walls meets the same way in the map's corner as anywhere
// else: the vertical wall stops a half thickness short of the
// horizontal one's centreline instead of crossing it.
#[test]
fn walls_meet_flush_in_every_corner_of_the_map() {
    use crate::test_geometry::geometry;

    let geometry = geometry(4, 4);
    let half = geometry.wall_half_thickness();
    let corners = [
        ((0, 0), (0, 0), 1.0),
        ((0, 3), (0, 4), -1.0),
        ((3, 0), (4, 0), 1.0),
        ((3, 3), (4, 4), -1.0),
    ];
    for ((col, row), (h_col, h_row), toward) in corners {
        let mut edges = EdgeGrid::new(4, 4);
        edges.horizontal[h_row as usize][col as usize] = true;
        edges.vertical[row as usize][h_col as usize] = true;
        let walls = generate_walls(&edges, &geometry, 0, CarrierId::WORLD);
        let vertical = walls
            .iter()
            .find(|wall| (wall.x1 - wall.x2).abs() < MERGE_EPS)
            .expect("vertical wall missing");
        let line_z = geometry.cell_to_world_z(h_row);
        let end = if toward > 0.0 { vertical.z1 } else { vertical.z2 };
        assert!(
            (end - (line_z + toward * half)).abs() < MERGE_EPS,
            "corner ({col}, {row}): vertical wall ends at {end}, expected {}",
            line_z + toward * half
        );
    }
}

#[test]
fn horizontal_walls_merge_when_only_hidden_caps_differ() {
    // Two adjacent horizontal wall edges. Long faces and top/bottom match;
    // their abutting east/west caps differ — those become interior on
    // merge and shouldn't block it.
    let left = (
        h_wall(0.0, 1.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "INNER_E", "outer_W"),
    );
    let right = (
        h_wall(1.0, 2.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "outer_E", "INNER_W"),
    );

    let (walls, materials) = merge_walls_with_materials(vec![left, right]);

    assert_eq!(walls.len(), 1);
    assert!((walls[0].x1 - 0.0).abs() < MERGE_EPS);
    assert!((walls[0].x2 - 2.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].west, "outer_W");
    assert_eq!(materials[0].east, "outer_E");
}

#[test]
fn horizontal_walls_do_not_merge_when_visible_face_differs() {
    let left = (
        h_wall(0.0, 1.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
    );
    // North face differs — it's a long visible face for a horizontal wall.
    let right = (
        h_wall(1.0, 2.0, 0.0),
        FaceMaterials::from_six("t", "b", "DIFFERENT", "s", "e", "w"),
    );

    let (walls, _) = merge_walls_with_materials(vec![left, right]);

    assert_eq!(walls.len(), 2);
}

#[test]
fn vertical_walls_merge_when_only_hidden_caps_differ() {
    let north = (
        v_wall(0.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "outer_N", "INNER_S", "e", "w"),
    );
    let south = (
        v_wall(0.0, 1.0, 2.0),
        FaceMaterials::from_six("t", "b", "INNER_N", "outer_S", "e", "w"),
    );

    let (walls, materials) = merge_walls_with_materials(vec![north, south]);

    assert_eq!(walls.len(), 1);
    assert!((walls[0].z1 - 0.0).abs() < MERGE_EPS);
    assert!((walls[0].z2 - 2.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].north, "outer_N");
    assert_eq!(materials[0].south, "outer_S");
}

#[test]
fn vertical_walls_do_not_merge_when_visible_face_differs() {
    let north = (
        v_wall(0.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
    );
    // East face differs — it's a long visible face for a vertical wall.
    let south = (
        v_wall(0.0, 1.0, 2.0),
        FaceMaterials::from_six("t", "b", "n", "s", "DIFFERENT", "w"),
    );

    let (walls, _) = merge_walls_with_materials(vec![north, south]);

    assert_eq!(walls.len(), 2);
}

#[test]
fn three_horizontal_walls_chain_with_outer_caps() {
    let a = (
        h_wall(0.0, 1.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "ae", "leftmost_W"),
    );
    let b = (
        h_wall(1.0, 2.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "be", "bw"),
    );
    let c = (
        h_wall(2.0, 3.0, 0.0),
        FaceMaterials::from_six("t", "b", "n", "s", "rightmost_E", "cw"),
    );

    let (walls, materials) = merge_walls_with_materials(vec![a, b, c]);

    assert_eq!(walls.len(), 1);
    assert!((walls[0].x1 - 0.0).abs() < MERGE_EPS);
    assert!((walls[0].x2 - 3.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].west, "leftmost_W");
    assert_eq!(materials[0].east, "rightmost_E");
}
