use super::*;

#[test]
fn plate_box_top_spans_its_tile_and_has_tangents() {
    let mesh = plate_box(1.2, 0.05, 1.2);
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) => uvs,
        other => panic!("unexpected UV attribute: {other:?}"),
    };
    let min_u = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
    let max_u = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
    assert!(
        (max_u - min_u - 1.0).abs() < 1e-5,
        "one tile across the panel: {min_u}..{max_u}"
    );
    assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
}
