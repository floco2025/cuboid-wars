use super::{super::traversal::PORTAL_PROJECTILE_EXIT_STANDOFF, *};
use crate::physics::{ProjectileEvent, earliest_projectile_event};

#[test]
fn projectile_hop_requires_front_side_approach() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let toward = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
    assert!(toward.is_some());
    let from_behind = set.projectile_hop(Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.0, 0.0, 30.0), 0.1, 0.08);
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
        .projectile_hop(Vec3::new(0.2, 1.1, 2.0), velocity, 0.1, 0.08)
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
            .projectile_hop(start, Vec3::Z * speed, TICK_SECS, 0.08)
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
        set.projectile_hop(Vec3::new(0.0, 1.0, 0.15), travel / TICK_SECS, TICK_SECS, 0.08)
            .is_none()
    );
}

#[test]
fn a_sliding_portal_uses_its_aperture_at_the_crossing_time() {
    let travel = Vec3::X * (4.0 * PORTAL_HALF_WIDTH);
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    let velocity = Vec3::NEG_Z * 6.0;
    let hop = set
        .projectile_hop(Vec3::new(travel.x / 2.0, 1.0, 0.18), velocity, TICK_SECS, 0.08)
        .expect("projectile missed the sliding aperture at mid-tick");
    assert!((hop.t - 0.5).abs() < 1e-5);
    assert!((hop.exit_pos.x - 10.0).abs() < 1e-5, "{hop:?}");
    assert!(
        set.projectile_hop(Vec3::new(0.0, 1.0, 0.18), velocity, TICK_SECS, 0.08)
            .is_none()
    );
}

#[test]
fn a_projectile_segment_uses_only_the_portals_remaining_tick_travel() {
    let (_, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.27), Vec3::ZERO, TICK_SECS / 4.0, 0.08)
        .expect("portal missed the projectile during the final quarter tick");
    assert!((hop.t - 0.8).abs() < 1e-5, "{hop:?}");
}

#[test]
fn a_projectile_emerges_at_the_moving_exits_crossing_time() {
    let travel = Vec3::new(0.4, 0.2, -0.1);
    let (_, set) = moving_projectile_portals(Vec3::ZERO, travel, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.18), Vec3::NEG_Z * 6.0, TICK_SECS, 0.08)
        .expect("projectile missed the static entry portal");
    assert!((hop.t - 0.5).abs() < 1e-5);
    let expected = Vec3::new(10.0, 1.0, 0.0) + travel * 0.5 + Vec3::Z * (0.08 + PORTAL_PROJECTILE_EXIT_STANDOFF);
    assert!(hop.exit_pos.abs_diff_eq(expected, 1e-5), "{hop:?}");
}

#[test]
fn an_approaching_portals_backing_wall_does_not_win_a_premature_bounce() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let start = Vec3::new(0.0, 1.0, 1.0);
    let velocity = Vec3::NEG_Z * 90.0;
    let hop = set
        .projectile_hop(start, velocity, TICK_SECS, 0.08)
        .expect("projectile missed the portal");
    let surface = world.cast_bouncing_ball_excluding(start, velocity * TICK_SECS, 0.08, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), Some(hop.t)),
        ProjectileEvent::Portal
    );

    let outside = Vec3::new(PORTAL_HALF_WIDTH * 2.0, 1.0, 1.0);
    assert!(set.projectile_hop(outside, velocity, TICK_SECS, 0.08).is_none());
    let surface = world.cast_moving_ball(outside, velocity * TICK_SECS, 0.08);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), None),
        ProjectileEvent::Surface
    );
}

#[test]
fn a_valid_portal_crossing_still_bounces_off_an_unrelated_obstacle() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.18, 0.02)]);
    let start = Position { x: 0.0, y: 1.0, z: 1.0 };
    let mut projectile = portal_test_projectile(Vec3::NEG_Z * 90.0);
    let hop = set
        .projectile_hop(start.into(), projectile.velocity, TICK_SECS, 0.08)
        .expect("projectile missed the portal");
    let surface_t = projectile.surface_collision_t(&start, TICK_SECS, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Surface
    );

    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, hop.entry_backing)
        .expect("obstacle did not bounce the projectile");
    assert!((bounce.position.z - 0.27).abs() < 1e-4, "{bounce:?}");
    assert!(projectile.velocity.z > 0.0);
}

#[test]
fn a_bounced_projectile_crosses_a_moving_portal_during_the_same_tick() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.83, 0.2)]);
    let start = Position { x: 0.0, y: 1.0, z: 0.4 };
    let mut projectile = portal_test_projectile(Vec3::Z * 30.0);
    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, &[])
        .expect("projectile missed the bounce wall");
    assert!((bounce.remaining_delta / TICK_SECS - 0.75).abs() < 1e-4);

    let hop = set
        .projectile_hop(
            bounce.position.into(),
            projectile.velocity,
            bounce.remaining_delta,
            0.08,
        )
        .expect("bounced projectile missed the portal");
    let crossing_tick = 1.0 - bounce.remaining_delta / TICK_SECS * (1.0 - hop.t);
    assert!((hop.entry_point.z - 0.2 * crossing_tick - 0.08).abs() < 1e-4, "{hop:?}");
    let surface_t = projectile.surface_collision_t(&bounce.position, bounce.remaining_delta, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Portal
    );
}

#[test]
fn half_placed_pair_is_inert() {
    let set = PortalSet::rebuild(
        &[portal(PortalEnd::A, Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0)],
        &empty_world(),
        &Carriers::default(),
    );
    assert!(set.is_empty());
    let hop = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
    assert!(hop.is_none());
}
