use super::*;

#[test]
fn flag_triangles_face_the_same_way_as_their_normals() {
    let mesh = pennant_mesh(1.2, 0.7, 16);
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("flag positions missing");
    let indices: Vec<_> = mesh.indices().expect("flag indices missing").iter().collect();
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| Vec3::from_array(positions[i]));
        assert!((b - a).cross(c - a).dot(Vec3::Z) > 0.0);
    }
}
