use super::*;
use bevy::mesh::VertexAttributeValues;

#[test]
fn tree_detail_levels_have_valid_surfaces_and_reduce_geometry() {
    for variant in 0..TREE_VARIANTS {
        let tree = Tree::grow(variant);
        let mut previous_triangles = usize::MAX;
        for lod in 0..3 {
            let (wood, leaves) = tree.meshes(lod);
            let mut triangles = 0;
            for mesh in [&wood, &leaves] {
                let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                    panic!("tree positions missing");
                };
                let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
                    panic!("tree normals missing");
                };
                assert!(positions.iter().all(|p| Vec3::from_array(*p).is_finite()));
                assert!(
                    normals
                        .iter()
                        .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 0.001)
                );
                let indices: Vec<_> = mesh.indices().expect("tree indices missing").iter().collect();
                for face in indices.chunks_exact(3) {
                    let [a, b, c] = [face[0], face[1], face[2]].map(|i| Vec3::from_array(positions[i]));
                    assert!((b - a).cross(c - a).length_squared() > 1e-12);
                }
                triangles += indices.len() / 3;
            }
            assert!(triangles < previous_triangles);
            previous_triangles = triangles;
        }
    }
}
