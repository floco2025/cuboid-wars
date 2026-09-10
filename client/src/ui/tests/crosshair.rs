use super::*;

#[test]
fn pattern_reticle_keeps_one_full_size_center_and_normalizes_the_others() {
    let offsets = pattern_reticle_offsets(&[(0.1, 0.0), (0.0, 0.0), (-0.1, 0.05)]);
    assert_eq!(offsets.iter().filter(|(_, center)| *center).count(), 1);
    assert_eq!(offsets[1], (Vec2::ZERO, true));
    assert_eq!(offsets[0].0, Vec2::new(-PATTERN_RADIUS_PX, 0.0));
    assert_eq!(offsets[2].0, Vec2::new(PATTERN_RADIUS_PX, -PATTERN_RADIUS_PX / 2.0));
}

#[test]
fn portal_reticle_colors_match_the_assigned_ends() {
    assert_eq!(portal_color(PortalEnd::A), PORTAL_A_COLOR);
    assert_eq!(portal_color(PortalEnd::B), PORTAL_B_COLOR);
}
