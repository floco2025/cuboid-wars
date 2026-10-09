use bevy::mesh::VertexAttributeValues;
use common::protocol::{CarrierId, Ladder};

use super::*;
use crate::test_fixtures::LEVEL_HEIGHT;

#[test]
fn ladder_mesh_spreads_its_uvs_and_has_valid_tangents() {
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
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("ladder UVs missing or not Float32x2");
    };
    let min_u = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
    let max_u = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
    let max_v = uvs.iter().map(|uv| uv[1]).fold(f32::MIN, f32::max);
    // World-anchored: members must spread across the texture, not all
    // sample the same edge sliver.
    assert!(max_u - min_u > 1.0, "UVs bunched: {min_u}..{max_u}");
    assert!(max_v > 1.0, "rail faces should tile vertically: max_v={max_v}");
    assert_eq!(
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).map(VertexAttributeValues::len),
        Some(uvs.len()),
        "attribute counts diverge"
    );
    let Some(VertexAttributeValues::Float32x4(tangents)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT) else {
        panic!("ladder tangents missing or not Float32x4");
    };
    let bad = tangents
        .iter()
        .filter(|t| {
            let len = Vec3::new(t[0], t[1], t[2]).length();
            !len.is_finite() || len < 0.5 || !t[3].is_finite() || t[3].abs() < 0.5
        })
        .count();
    assert_eq!(
        bad,
        0,
        "{bad}/{} degenerate tangents, first: {:?}",
        tangents.len(),
        &tangents[..4]
    );
}
