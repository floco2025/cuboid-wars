use super::*;
use bevy::mesh::Indices;

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
