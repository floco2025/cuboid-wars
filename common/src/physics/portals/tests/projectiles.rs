use super::{super::traversal::PORTAL_PROJECTILE_EXIT_STANDOFF, *};

#[test]
fn projectile_hop_requires_front_side_approach() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let toward = set.projectile_hop(
        Vec3::new(0.0, 1.0, 2.0),
        Vec3::new(0.0, 0.0, -30.0),
        0.1,
        0.08,
        TICK_SECS,
    );
    assert!(toward.is_some());
    let from_behind = set.projectile_hop(
        Vec3::new(0.0, 1.0, -2.0),
        Vec3::new(0.0, 0.0, 30.0),
        0.1,
        0.08,
        TICK_SECS,
    );
    assert!(from_behind.is_none());
}

#[test]
fn projectile_hop_rejects_shots_outside_the_aperture() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.projectile_hop(
        Vec3::new(2.0 * PORTAL_HALF_WIDTH, 1.0, 2.0),
        Vec3::new(0.0, 0.0, -30.0),
        0.1,
        0.08,
        TICK_SECS,
    );
    assert!(hop.is_none());
}

#[test]
fn projectile_continues_straight_through_a_facing_pair() {
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::Z,
        Vec3::new(0.0, 1.0, 10.0),
        Vec3::NEG_Z,
    );
    let velocity = Vec3::new(1.0, 0.0, -30.0);
    let hop = set
        .projectile_hop(Vec3::new(0.2, 1.1, 2.0), velocity, 0.1, 0.08, TICK_SECS)
        .expect("projectile aimed at the portal did not hop");
    // A facing pair is a tunnel: the lateral offset and velocity carry over.
    assert!((hop.exit_velocity - velocity).length() < 1e-4);
    assert!((hop.exit_pos.x - hop.entry_point.x).abs() < 1e-4);
    assert!((hop.exit_pos.y - hop.entry_point.y).abs() < 1e-4);
    assert!((hop.exit_velocity.length() - velocity.length()).abs() < 1e-4);
}

#[test]
fn an_approaching_portal_catches_a_projectile_it_sweeps_across() {
    let travel = Vec3::Z * 0.2;
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    for speed in [-0.1, 0.0, 0.1] {
        let start = Vec3::new(0.0, 1.0, 0.15);
        let hop = set
            .projectile_hop(start, Vec3::Z * speed, TICK_SECS, 0.08, TICK_SECS)
            .expect("approaching portal missed the projectile");
        assert!((hop.entry_point.z - travel.z * hop.t - 0.08).abs() < 1e-5, "{hop:?}");
        assert!(
            hop.entry_point
                .abs_diff_eq(start + Vec3::Z * speed * TICK_SECS * hop.t, 1e-5)
        );
    }
}

#[test]
fn a_retreating_portal_does_not_catch_a_projectile_that_keeps_its_distance() {
    let travel = Vec3::NEG_Z * 0.2;
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    assert!(
        set.projectile_hop(
            Vec3::new(0.0, 1.0, 0.15),
            travel / TICK_SECS,
            TICK_SECS,
            0.08,
            TICK_SECS
        )
        .is_none()
    );
}

#[test]
fn a_sliding_portal_uses_its_aperture_at_the_crossing_time() {
    let travel = Vec3::X * (4.0 * PORTAL_HALF_WIDTH);
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    let velocity = Vec3::NEG_Z * 6.0;
    let hop = set
        .projectile_hop(
            Vec3::new(travel.x / 2.0, 1.0, 0.18),
            velocity,
            TICK_SECS,
            0.08,
            TICK_SECS,
        )
        .expect("projectile missed the sliding aperture at mid-tick");
    assert!((hop.t - 0.5).abs() < 1e-5);
    assert!((hop.exit_pos.x - 10.0).abs() < 1e-5, "{hop:?}");
    assert!(
        set.projectile_hop(Vec3::new(0.0, 1.0, 0.18), velocity, TICK_SECS, 0.08, TICK_SECS)
            .is_none()
    );
}

#[test]
fn a_projectile_segment_uses_only_the_portals_remaining_tick_travel() {
    let (_, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.27), Vec3::ZERO, TICK_SECS / 4.0, 0.08, TICK_SECS)
        .expect("portal missed the projectile during the final quarter tick");
    assert!((hop.t - 0.8).abs() < 1e-5, "{hop:?}");
}

#[test]
fn a_projectile_emerges_at_the_moving_exits_crossing_time() {
    let travel = Vec3::new(0.4, 0.2, -0.1);
    let (_, set) = moving_projectile_portals(Vec3::ZERO, travel, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.18), Vec3::NEG_Z * 6.0, TICK_SECS, 0.08, TICK_SECS)
        .expect("projectile missed the static entry portal");
    assert!((hop.t - 0.5).abs() < 1e-5);
    let expected = Vec3::new(10.0, 1.0, 0.0) + travel * 0.5 + Vec3::Z * (0.08 + PORTAL_PROJECTILE_EXIT_STANDOFF);
    assert!(hop.exit_pos.abs_diff_eq(expected, 1e-5), "{hop:?}");
}

#[test]
fn half_placed_pair_is_inert() {
    let set = PortalSet::rebuild(
        &[portal(PortalEnd::A, Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0)],
        &empty_world(),
        &Carriers::default(),
    );
    assert!(set.is_empty());
    let hop = set.projectile_hop(
        Vec3::new(0.0, 1.0, 2.0),
        Vec3::new(0.0, 0.0, -30.0),
        0.1,
        0.08,
        TICK_SECS,
    );
    assert!(hop.is_none());
}
