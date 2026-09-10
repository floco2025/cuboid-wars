use super::*;
use crate::test_fixtures::LEVEL_HEIGHT;
use common::protocol::{CarrierId, Ladder};

#[test]
fn ladder_mesh_has_varied_uvs_and_tangents() {
    let ladder = Ladder {
        x1: -0.5,
        z1: 0.0,
        x2: 0.5,
        z2: 0.0,
        nx: 0.0,
        nz: -1.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    };
    let mesh = build_ladder_mesh(&ladder, 0.6);
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) => uvs,
        other => panic!("unexpected UV attribute: {other:?}"),
    };
    let min_u = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
    let max_u = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
    let max_v = uvs.iter().map(|uv| uv[1]).fold(f32::MIN, f32::max);
    // World-anchored: members must spread across the texture, not all
    // sample the same edge sliver.
    assert!(max_u - min_u > 1.0, "UVs bunched: {min_u}..{max_u}");
    assert!(max_v > 1.0, "rail faces should tile vertically: max_v={max_v}");
    assert!(
        mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(),
        "tangent generation failed"
    );
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .map(bevy::mesh::VertexAttributeValues::len);
    let uv_len = uvs.len();
    assert_eq!(positions, Some(uv_len), "attribute counts diverge");
}

#[test]
fn ladder_mesh_tangents_are_finite_and_nonzero() {
    let ladder = Ladder {
        x1: -0.5,
        z1: 0.0,
        x2: 0.5,
        z2: 0.0,
        nx: 0.0,
        nz: -1.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    };
    let mesh = build_ladder_mesh(&ladder, 0.6);
    let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
        Some(bevy::mesh::VertexAttributeValues::Float32x4(t)) => t,
        other => panic!("unexpected tangent attribute: {other:?}"),
    };
    let mut bad = 0;
    for t in tangents {
        let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
        if !len.is_finite() || len < 0.5 || !t[3].is_finite() || t[3].abs() < 0.5 {
            bad += 1;
        }
    }
    assert_eq!(
        bad,
        0,
        "{bad}/{} degenerate tangents, first: {:?}",
        tangents.len(),
        &tangents[..4]
    );
}
