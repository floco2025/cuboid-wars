use super::*;
use rerecast::{AreaType, Contour, RegionId};

#[test]
fn crossing_simplified_boundary_recovers_without_filling_obstacles_or_separating_neighbors() {
    let points = |points: &[(u16, u16)]| {
        points
            .iter()
            .map(|&(x, z)| (U16Vec3::new(x, 0, z), 0))
            .collect::<Vec<_>>()
    };
    let simplified = points(&[
        (28, 6),
        (0, 135),
        (0, 136),
        (1, 136),
        (1, 138),
        (2, 138),
        (2, 140),
        (3, 140),
        (3, 142),
        (4, 142),
        (4, 148),
        (3, 148),
        (3, 149),
        (97, 51),
        (46, 0),
        (45, 2),
        (44, 2),
        (44, 3),
        (40, 4),
        (40, 5),
        (38, 5),
        (38, 6),
        (36, 6),
        (36, 7),
        (29, 7),
        (29, 6),
    ]);
    let mut raw = simplified.clone();
    raw.insert(13, (U16Vec3::new(7, 0, 149), 0));
    let neighbor = points(&[(3, 149), (110, 149), (110, 51), (97, 51)]);
    let mut neighbor_raw = neighbor.clone();
    neighbor_raw.push((U16Vec3::new(7, 0, 149), 0));
    let hole = points(&[(100, 80), (105, 80), (105, 85), (100, 85)]);
    let contour = |vertices, raw: &[Vertex], region| Contour {
        vertices,
        raw_vertices: raw
            .iter()
            .map(|&(point, flags)| (point, RegionVertexId::from_bits_retain(flags)))
            .collect(),
        region: RegionId::from_bits_retain(region),
        area: AreaType::DEFAULT_WALKABLE,
    };
    let set = ContourSet {
        contours: vec![
            contour(simplified, &raw, 1),
            contour(neighbor, &neighbor_raw, 2),
            contour(hole.clone(), &hole, 2),
        ],
        cell_size: 1.0,
        cell_height: 0.1,
        ..Default::default()
    };
    assert!(
        triangulate(set.clone(), 6).is_err(),
        "fixture must reproduce triangulation failure"
    );
    let mesh = polygon_mesh(set, 6).expect("recover the unsimplified outline");
    let area: i64 = mesh
        .polygons()
        .map(|polygon| {
            let vertices: Vec<_> = polygon.map(|index| (mesh.vertices[usize::from(index)], 0)).collect();
            assert!(!inside(&vertices, (2.5, 145.0)), "obstacle notch filled");
            assert!(!inside(&vertices, (102.5, 82.5)), "enclosed obstacle filled");
            signed_area(&vertices).abs()
        })
        .sum();
    assert_eq!(
        area,
        signed_area(&raw).abs() + signed_area(&neighbor_raw).abs() - signed_area(&hole).abs()
    );
    assert!(
        mesh.polygon_neighbors
            .chunks(usize::from(mesh.max_vertices_per_polygon))
            .enumerate()
            .any(|(index, neighbors)| {
                neighbors.iter().any(|&neighbor| {
                    neighbor != PolygonNavmesh::NO_CONNECTION
                        && mesh.regions[index] != mesh.regions[usize::from(neighbor)]
                })
            }),
        "the two sides of the repaired boundary must remain connected"
    );
}

#[test]
fn enclosed_obstacles_remain_holes_after_polygon_triangulation() {
    let contour = |points: &[(u16, u16)]| Contour {
        vertices: points.iter().map(|&(x, z)| (U16Vec3::new(x, 0, z), 0)).collect(),
        region: RegionId::from_bits_retain(1),
        area: AreaType::DEFAULT_WALKABLE,
        ..Default::default()
    };
    let set = ContourSet {
        contours: vec![
            contour(&[(0, 0), (0, 20), (20, 20), (20, 0)]),
            contour(&[(3, 3), (7, 3), (7, 7), (3, 7)]),
            contour(&[(12, 12), (17, 12), (17, 17), (12, 17)]),
        ],
        cell_size: 1.0,
        cell_height: 0.1,
        ..Default::default()
    };
    let polygons = polygon_mesh(set, 6).expect("triangulate connected contours");
    let mut area = 0_i64;
    for polygon in polygons.polygons() {
        let vertices: Vec<_> = polygon
            .map(|index| (polygons.vertices[usize::from(index)], 0))
            .collect();
        area += signed_area(&vertices).abs();
        assert!(!inside(&vertices, (5.0, 5.0)));
        assert!(!inside(&vertices, (14.0, 14.0)));
    }
    assert_eq!(area, 2 * (400 - 16 - 25));
}
