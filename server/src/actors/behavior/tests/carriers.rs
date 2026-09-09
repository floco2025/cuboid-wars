use super::*;

fn carrier_rest() -> Position {
    Position {
        x: 40.0,
        y: LEVEL_HEIGHT,
        z: -30.0,
    }
}

#[test]
fn carried_actor_roams_in_its_carriers_frame() {
    let fixture = Fixture::with_carrier(CONTACT, carrier_rest());
    let mut info = info(CONTACT);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, fixture.pos(1, 2)), &mut rng);

    assert_eq!(info.mode, ActorMode::Roam);
    let route = info.route.expect("a roam route");
    for waypoint in &route.waypoints {
        assert!(
            fixture.graph().contains(&waypoint.position),
            "{waypoint:?} is off the carrier's grid"
        );
        assert_eq!(waypoint.position.y, 0.0);
    }
}

#[test]
fn player_off_the_carrier_is_unreachable() {
    let fixture = Fixture::with_carrier(CONTACT, carrier_rest());
    let mut info = info(CONTACT);
    let context = fixture.context(CONTACT, fixture.pos(1, 2));
    let beside = Position {
        x: carrier_rest().x + fixture.geometry.width() / 2.0 + CELL,
        ..carrier_rest()
    };
    let underneath = Position {
        y: carrier_rest().y - LEVEL_HEIGHT,
        ..context.world_pos
    };

    assert!(!keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        beside
    ));
    assert!(!keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        underneath
    ));
    assert!(info.route.is_none());
}

#[test]
fn player_aboard_the_carrier_is_engaged_along_a_carrier_local_route() {
    let fixture = Fixture::with_carrier(CONTACT, carrier_rest());
    let mut info = info(CONTACT);
    let context = fixture.context(CONTACT, fixture.pos(1, 2));
    let target_local = fixture.pos(8, 2);
    let target_world = fixture.pose().transform_position(&target_local);

    assert!(keep_or_install_engagement_route(
        &mut info,
        &context,
        PlayerId(1),
        target_world
    ));

    let route = info.route.as_ref().expect("an engagement route");
    assert!(
        route.destination.distance_sq(&target_local) < 1e-6,
        "{:?}",
        route.destination
    );
    assert!(
        route
            .waypoints
            .iter()
            .all(|waypoint| fixture.graph().contains(&waypoint.position))
    );
    assert_eq!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(1),
            target_pos: target_world
        }
    );
}

#[test]
fn cover_is_judged_at_the_candidates_world_position() {
    let fixture = Fixture::with_carrier(CONTACT, carrier_rest());
    let context = fixture.context(CONTACT, fixture.pos(1, 2));
    let candidate = fixture.pos(8, 2);
    // A threat standing on the candidate in the world; in an open world
    // nothing is ever cover, so only the too-close test can decide.
    let threat_on_it = fixture.pose().transform_position(&candidate);
    let threat_far = Position {
        x: threat_on_it.x + 100.0,
        ..threat_on_it
    };

    assert!(!context.stable_cover(&candidate, &[threat_on_it]));
    assert!(!context.stable_cover(&candidate, &[threat_far]));
}
