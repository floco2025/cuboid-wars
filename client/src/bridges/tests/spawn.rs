use super::*;
use crate::fields::field_pane_mesh;

#[test]
fn pane_corners_match_the_collision_footprint_at_the_walking_surface() {
    let surface = Rect::new(-0.2, -0.2, 4.2, 2.2);
    let center = surface.center();
    let transform = bridge_transform(center, 5.0);
    let mesh = field_pane_mesh(
        &[Rect {
            min: surface.min - center,
            max: surface.max - center,
        }],
        &transform,
        0.25,
    );
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("pane positions missing");
    let corners: Vec<Vec3> = positions
        .iter()
        .map(|p| transform.transform_point(Vec3::from_array(*p)))
        .collect();
    for (x, z) in [(-0.2, -0.2), (4.2, -0.2), (4.2, 2.2), (-0.2, 2.2)] {
        assert!(
            corners
                .iter()
                .any(|corner| corner.abs_diff_eq(Vec3::new(x, 5.0, z), 1e-5)),
            "corner ({x}, {z}) missing from {corners:?}"
        );
    }
}
