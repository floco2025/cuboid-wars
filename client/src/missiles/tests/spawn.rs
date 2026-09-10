use super::*;

// The collision/fuse ball (`MISSILE_RADIUS`) is what keeps the
// missile clear of geometry; the rendered mesh must fit inside it
// radially or missiles visibly clip walls they fly along. The two are
// deliberately tuned separately — this pins the invariant, not a ratio.
#[test]
fn rendered_missile_fits_inside_the_collision_ball() {
    let widest_radial_extent = MISSILE_BODY_RADIUS + MISSILE_FIN_SPAN;
    assert!(
        widest_radial_extent <= crate::constants::MISSILE_RADIUS,
        "missile mesh ({widest_radial_extent} m radial) exceeds the collision ball ({} m)",
        crate::constants::MISSILE_RADIUS
    );
}

#[test]
fn missile_rotation_points_the_nose_along_the_velocity() {
    for velocity in [Vec3::X * 12.0, Vec3::new(3.0, -5.0, 8.0), Vec3::NEG_Y] {
        let nose = missile_rotation(velocity) * Vec3::Y;
        assert!(
            nose.dot(velocity.normalize()) > 0.9999,
            "nose {nose} should align with {velocity}"
        );
    }
}

#[test]
fn missile_rotation_zero_velocity_is_identity() {
    assert_eq!(missile_rotation(Vec3::ZERO), Quat::IDENTITY);
}
