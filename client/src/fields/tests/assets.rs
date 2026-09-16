use super::*;
use bevy::mesh::{Indices, MeshVertexAttribute, VertexAttributeValues};

#[test]
fn field_panel_is_a_unit_quad_with_vertex_colors() {
    let mut meshes = Assets::default();
    let field_meshes = FieldMeshes::new(&mut meshes);
    let mesh = meshes.get(&field_meshes.panel).expect("panel mesh missing");
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("panel mesh positions missing");
    assert_eq!(positions.len(), 4);
    assert!(
        positions
            .iter()
            .all(|p| p[0].abs() == 0.5 && p[1].abs() == 0.5 && p[2] == 0.0)
    );
    assert_eq!(mesh.indices().map(Indices::len), Some(6));
    assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR));
}

fn float2(mesh: &Mesh, attribute: MeshVertexAttribute) -> Vec<[f32; 2]> {
    match mesh.attribute(attribute).expect("pane attribute missing") {
        VertexAttributeValues::Float32x2(values) => values.clone(),
        other => panic!("pane attribute is not float2: {other:?}"),
    }
}

#[test]
fn pane_mesh_rings_the_rect_with_a_fade_band_in_carrier_frame_metres() {
    let root = Transform::from_xyz(10.0, 2.0, -4.0);
    let mesh = field_pane_mesh(Rect::new(-2.0, -1.0, 2.0, 1.0), &root, 0.25);
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("pane positions missing");
    assert_eq!(
        positions[..4],
        [[-2.0, -1.0, 0.0], [2.0, -1.0, 0.0], [2.0, 1.0, 0.0], [-2.0, 1.0, 0.0]]
    );
    assert_eq!(
        positions[4..],
        [
            [-1.75, -0.75, 0.0],
            [1.75, -0.75, 0.0],
            [1.75, 0.75, 0.0],
            [-1.75, 0.75, 0.0]
        ]
    );
    let band = float2(&mesh, Mesh::ATTRIBUTE_UV_1);
    assert!(band[..4].iter().all(|uv| uv[0] == 0.0) && band[4..].iter().all(|uv| uv[0] == 1.0));
    let pattern = float2(&mesh, Mesh::ATTRIBUTE_UV_0);
    assert_eq!(
        pattern[0],
        [8.0, 1.0],
        "pattern coordinates are the carrier-frame position"
    );
    assert_eq!(mesh.indices().map(Indices::len), Some(30));
    assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR));
}

#[test]
fn pane_mesh_inset_stops_at_half_the_short_side() {
    let mesh = field_pane_mesh(Rect::new(0.0, 0.0, 3.0, 0.2), &Transform::IDENTITY, 0.25);
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("pane positions missing");
    assert_eq!(positions[4], [0.1, 0.1, 0.0]);
    assert_eq!(positions[6], [2.9, 0.1, 0.0]);
}

#[test]
fn neighbouring_panes_share_pattern_coordinates_on_their_common_edge() {
    let left = field_pane_mesh(
        Rect::new(-1.0, -1.0, 1.0, 1.0),
        &Transform::from_xyz(1.0, 0.0, 0.0),
        0.1,
    );
    let right = field_pane_mesh(
        Rect::new(-1.5, -1.0, 1.5, 1.0),
        &Transform::from_xyz(3.5, 0.0, 0.0),
        0.1,
    );
    let (left, right) = (
        float2(&left, Mesh::ATTRIBUTE_UV_0),
        float2(&right, Mesh::ATTRIBUTE_UV_0),
    );
    assert_eq!(left[1], right[0]);
    assert_eq!(left[2], right[3]);
    let turned = field_pane_mesh(
        Rect::new(-1.0, -1.0, 1.0, 1.0),
        &Transform::from_xyz(0.0, 0.0, 5.0).with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        0.1,
    );
    assert!(
        float2(&turned, Mesh::ATTRIBUTE_UV_0)[0]
            .iter()
            .zip([4.0, -1.0])
            .all(|(a, b)| (a - b).abs() < 1e-5),
        "a pane turned along Z takes its coordinates from the carrier's Z"
    );
}
