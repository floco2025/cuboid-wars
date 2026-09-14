use super::*;
use bevy::mesh::VertexAttributeValues;

#[test]
fn procedural_surface_uses_compiled_floor_bounds_including_trim() {
    let floor = Floor {
        x1: -1.25,
        z1: -1.0,
        x2: 1.4,
        z2: 1.3,
        y: 2.0,
        thickness: 0.2,
        level: 1,
        carrier: CarrierId::WORLD,
    };
    let mesh = terrain_surface_mesh(&[floor], Vec3::new(0.0, 2.0, 0.0));
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        panic!("terrain surface positions missing");
    };
    assert_eq!(
        positions.as_slice(),
        &[[-1.25, 0.0, -1.0], [1.4, 0.0, -1.0], [-1.25, 0.0, 1.3], [1.4, 0.0, 1.3]]
    );
}
