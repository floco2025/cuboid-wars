use super::*;
use bevy::mesh::VertexAttributeValues;

#[test]
fn rock_meshes_have_creased_unit_normals_tangents_and_texture_coordinates() {
    for class in RockClass::ALL {
        for variant in 0..ROCK_VARIANTS {
            let mut previous_triangles = usize::MAX;
            for &subdivisions in near_subdivisions(class) {
                let shape = rock_shape(class, variant, subdivisions);
                let mesh = rock_mesh(&shape, 1.0, Color::WHITE, variant);
                let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                    panic!("rock positions missing");
                };
                let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
                    panic!("rock normals missing");
                };
                assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
                assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
                assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
                assert_eq!(positions.len(), shape.triangles.len() * 3);
                assert!(positions.iter().all(|p| Vec3::from_array(*p).is_finite()));
                for (index, normal) in normals.iter().enumerate() {
                    let normal = Vec3::from_array(*normal);
                    assert!((normal.length() - 1.0).abs() < 0.001);
                    // A vertex normal never turns away from its own face.
                    let face = index / 3;
                    let [a, b, c] = shape.triangles[face].map(|i| shape.vertices[i as usize]);
                    assert!(normal.dot((b - a).cross(c - a)) > 0.0);
                }
                let triangles = positions.len() / 3;
                assert!(triangles < previous_triangles);
                previous_triangles = triangles;
            }
        }
    }
}

#[test]
fn detail_levels_fade_in_order_and_pebbles_have_one() {
    for lod in 0..3 {
        let stone = lod_range(RockClass::Stone, lod);
        let boulder = lod_range(RockClass::Boulder, lod);
        for range in [&stone, &boulder] {
            assert!(range.start_margin.end <= range.end_margin.start);
        }
        if lod > 0 {
            assert_eq!(stone.start_margin, lod_range(RockClass::Stone, lod - 1).end_margin);
        }
        assert!(boulder.end_margin.start > stone.end_margin.start);
    }
    let pebble = lod_range(RockClass::Pebble, 0);
    assert_eq!(near_subdivisions(RockClass::Pebble).len(), 1);
    assert!(pebble.end_margin.end < lod_range(RockClass::Stone, 0).end_margin.end * 2.0);
}
