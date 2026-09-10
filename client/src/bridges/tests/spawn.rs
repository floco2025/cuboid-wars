use super::*;

#[test]
fn rectangle_matches_collision_footprint_at_the_walking_surface() {
    let surface = Rect::new(-0.2, -0.2, 4.2, 2.2);
    let transform =
        bridge_transform(surface.center(), 5.0).with_scale(Vec3::new(surface.width(), surface.height(), 1.0));
    for (local_x, x) in [(-0.5, -0.2), (0.5, 4.2)] {
        for (local_y, z) in [(-0.5, -0.2), (0.5, 2.2)] {
            let point = transform.transform_point(Vec3::new(local_x, local_y, 0.0));
            assert!(point.abs_diff_eq(Vec3::new(x, 5.0, z), 1e-5));
        }
    }
}
