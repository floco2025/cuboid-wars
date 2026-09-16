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

fn band_at(mesh: &Mesh, point: Vec2) -> f32 {
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("pane positions missing");
    let index = positions
        .iter()
        .position(|p| Vec2::new(p[0], p[1]).abs_diff_eq(point, 1e-5))
        .expect("pane point missing");
    float2(mesh, Mesh::ATTRIBUTE_UV_1)[index][0]
}

#[test]
fn adjacent_and_concave_panes_glow_only_at_the_exposed_outline() {
    let surfaces = [Rect::new(0.0, 0.0, 2.0, 4.0), Rect::new(2.0, 0.0, 4.0, 2.0)];
    let mesh = field_pane_mesh(&surfaces, &Transform::IDENTITY, 0.25);
    assert_eq!(band_at(&mesh, Vec2::new(2.0, 0.25)), 1.0);
    assert_eq!(band_at(&mesh, Vec2::new(2.0, 1.75)), 1.0);
    assert_eq!(band_at(&mesh, Vec2::new(2.0, 2.25)), 0.0);
    assert_eq!(band_at(&mesh, Vec2::new(2.25, 2.0)), 0.0);
    assert_eq!(band_at(&mesh, Vec2::new(1.75, 2.25)), 1.0);
}

#[test]
fn narrow_panes_cannot_reach_full_distance_from_the_outline() {
    let mesh = field_pane_mesh(&[Rect::new(0.0, 0.0, 3.0, 0.2)], &Transform::IDENTITY, 0.25);
    assert!(float2(&mesh, Mesh::ATTRIBUTE_UV_1).iter().all(|uv| uv[0] < 1.0));
}

#[test]
fn pane_patterns_use_carrier_coordinates_after_translation_and_rotation() {
    let rect = Rect::new(-1.0, -1.0, 1.0, 1.0);
    for root in [
        Transform::from_xyz(10.0, 2.0, -4.0),
        Transform::from_xyz(0.0, 0.0, 5.0).with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ] {
        let mesh = field_pane_mesh(&[rect], &root, 0.25);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("pane positions missing");
        for (position, uv) in positions.iter().zip(float2(&mesh, Mesh::ATTRIBUTE_UV_0)) {
            let carrier = root.transform_point(Vec3::from_array(*position));
            assert!((uv[0] - carrier.dot(root.rotation * Vec3::X)).abs() < 1e-5);
            assert!((uv[1] - carrier.dot(root.rotation * Vec3::Y)).abs() < 1e-5);
        }
    }
}
