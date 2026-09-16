use super::*;

#[test]
fn pennant_tapers_from_its_hoist_to_a_tip_at_half_height() {
    let mesh = pennant_mesh(0.9, 0.4, 3);
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("pennant positions missing");
    assert_eq!(positions.len(), 8);
    assert_eq!(positions[0], [0.0, 0.0, 0.0]);
    assert_eq!(positions[1], [0.0, -0.4, 0.0]);
    assert_eq!(positions[6], [0.9, -0.2, 0.0]);
    assert_eq!(positions[7], [0.9, -0.2, 0.0]);
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("pennant reach missing") {
        bevy::mesh::VertexAttributeValues::Float32x2(values) => values.clone(),
        other => panic!("pennant reach is not float2: {other:?}"),
    };
    assert_eq!(uvs[0][0], 0.0);
    assert_eq!(uvs[7][0], 1.0);
    assert!(
        mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1),
        "weave coordinates missing"
    );
    assert_eq!(mesh.indices().map(bevy::mesh::Indices::len), Some(18));
}

#[test]
fn pennants_show_the_players_own_claim_and_the_groups() {
    let own = CheckpointPennant { index: 2, group: false };
    let group = CheckpointPennant { index: 2, group: true };
    assert!(pennant_claimed(&own, Some(2), None));
    assert!(!pennant_claimed(&own, Some(1), Some(2)));
    assert!(pennant_claimed(&group, None, Some(2)));
    assert!(!pennant_claimed(&group, Some(2), None));
}
