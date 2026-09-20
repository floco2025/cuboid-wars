use super::*;
use rerecast::{AreaType, Contour, RegionId};

#[test]
fn enclosed_obstacles_remain_holes_after_polygon_triangulation() {
    let contour = |points: &[(u16, u16)]| Contour {
        vertices: points.iter().map(|&(x, z)| (U16Vec3::new(x, 0, z), 0)).collect(),
        region: RegionId::from_bits_retain(1),
        area: AreaType::DEFAULT_WALKABLE,
        ..Default::default()
    };
    let mut set = ContourSet {
        contours: vec![
            contour(&[(0, 0), (0, 20), (20, 20), (20, 0)]),
            contour(&[(3, 3), (7, 3), (7, 7), (3, 7)]),
            contour(&[(12, 12), (17, 12), (17, 17), (12, 17)]),
        ],
        cell_size: 1.0,
        cell_height: 0.1,
        ..Default::default()
    };
    join_holes(&mut set).expect("join obstacle outlines");
    let polygons = set.into_polygon_mesh(6).expect("triangulate connected contours");
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
