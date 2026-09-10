use super::*;
use crate::test_geometry::FLOOR_THICKNESS;

fn rect(x1: f32, x2: f32, z1: f32, z2: f32) -> Floor {
    Floor {
        x1,
        x2,
        z1,
        z2,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn floors_merge_across_x_when_only_hidden_caps_differ() {
    let left = (
        rect(0.0, 1.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n", "s", "INNER", "outer_W"),
    );
    let right = (
        rect(1.0, 2.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n", "s", "outer_E", "INNER"),
    );

    let (floors, materials) = merge_floors_with_materials(vec![left, right]);

    assert_eq!(floors.len(), 1);
    assert!((floors[0].x1 - 0.0).abs() < MERGE_EPS);
    assert!((floors[0].x2 - 2.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].west, "outer_W");
    assert_eq!(materials[0].east, "outer_E");
}

#[test]
fn floors_do_not_merge_across_x_when_visible_face_differs() {
    let left = (
        rect(0.0, 1.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n", "s", "e", "w"),
    );
    // North is a visible long face when merging in x.
    let right = (
        rect(1.0, 2.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "DIFFERENT", "s", "e", "w"),
    );

    let (floors, _) = merge_floors_with_materials(vec![left, right]);

    assert_eq!(floors.len(), 2);
}

#[test]
fn floors_merge_across_z_when_only_hidden_caps_differ() {
    let north_rect = (
        rect(0.0, 1.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "outer_N", "INNER", "e", "w"),
    );
    let south_rect = (
        rect(0.0, 1.0, 1.0, 2.0),
        FaceMaterials::from_six("t", "b", "INNER", "outer_S", "e", "w"),
    );

    let (floors, materials) = merge_floors_with_materials(vec![north_rect, south_rect]);

    assert_eq!(floors.len(), 1);
    assert!((floors[0].z1 - 0.0).abs() < MERGE_EPS);
    assert!((floors[0].z2 - 2.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].north, "outer_N");
    assert_eq!(materials[0].south, "outer_S");
}

#[test]
fn floors_chain_x_then_z_with_correct_outer_caps() {
    // A,B abut along x; AB then abuts the row below (C,D) along z.
    // Each cell has distinct east/west AND distinct north/south, but the
    // north of A == north of B (visible during x merge), and the
    // east/west of (AB) need to match east/west of (CD) for the z merge.
    let a = (
        rect(0.0, 1.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n_top", "s_inner", "e_inner", "outer_W"),
    );
    let b = (
        rect(1.0, 2.0, 0.0, 1.0),
        FaceMaterials::from_six("t", "b", "n_top", "s_inner", "outer_E", "e_inner"),
    );
    let c = (
        rect(0.0, 1.0, 1.0, 2.0),
        FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "e_inner", "outer_W"),
    );
    let d = (
        rect(1.0, 2.0, 1.0, 2.0),
        FaceMaterials::from_six("t", "b", "n_inner", "s_bot", "outer_E", "e_inner"),
    );

    let (floors, materials) = merge_floors_with_materials(vec![a, b, c, d]);

    assert_eq!(floors.len(), 1);
    assert!((floors[0].x1 - 0.0).abs() < MERGE_EPS);
    assert!((floors[0].x2 - 2.0).abs() < MERGE_EPS);
    assert!((floors[0].z1 - 0.0).abs() < MERGE_EPS);
    assert!((floors[0].z2 - 2.0).abs() < MERGE_EPS);
    assert_eq!(materials[0].west, "outer_W");
    assert_eq!(materials[0].east, "outer_E");
    assert_eq!(materials[0].north, "n_top");
    assert_eq!(materials[0].south, "s_bot");
}
