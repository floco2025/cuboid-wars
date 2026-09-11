use super::*;

#[test]
fn actor_already_in_stable_cover_holds_position() {
    let open = Fixture::new(CONTACT);
    let actor_pos = open.pos(4, 2);
    let threat = open.pos(1, 2);
    let wall_x = (open.pos(2, 2).x + open.pos(3, 2).x) / 2.0;
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: wall_x,
            z1: actor_pos.z - 3.0,
            x2: wall_x,
            z2: actor_pos.z + 3.0,
            width: 0.2,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..MapLayout::default()
    });
    let fixture = Fixture::with_world(CONTACT, world);
    let mut info = info(CONTACT);
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, false));

    enter_evade(
        &mut info,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
    assert!(info.route.is_none());
}

#[test]
fn evade_route_is_replaced_when_same_cell_threat_exposes_destination() {
    let open = Fixture::new(CONTACT);
    let actor_pos = open.pos(5, 2);
    let destination = open.pos(4, 2);
    let protected_threat = open.pos(1, 2);
    let exposed_threat = Position {
        z: protected_threat.z + 1.4,
        ..protected_threat
    };
    let wall_x = (open.pos(2, 2).x + open.pos(3, 2).x) / 2.0;
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: wall_x,
            z1: destination.z - 0.5,
            x2: wall_x,
            z2: destination.z + 0.5,
            width: 0.2,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..MapLayout::default()
    });
    let fixture = Fixture::with_world(CONTACT, world);
    let context = fixture.context(CONTACT, actor_pos);
    assert!(context.stable_cover(&destination, &[protected_threat]));
    assert!(!context.stable_cover(&destination, &[exposed_threat]));
    assert_eq!(
        fixture.graph().nearest_node_for_position(&protected_threat),
        fixture.graph().nearest_node_for_position(&exposed_threat)
    );

    let mut info = info(CONTACT);
    info.mode = ActorMode::Evade { fleeing: false };
    info.route = Some(ActorRoute {
        waypoints: [destination].map(NavWaypoint::walk).into(),
        destination,
        destination_node: fixture
            .graph()
            .nearest_node_for_position(&destination)
            .expect("destination nav node"),
    });
    info.awareness
        .push(aware(7, exposed_threat, CharacterSupport::Ladder, true));

    enter_evade(&mut info, &context, &mut StdRng::seed_from_u64(1));

    assert!(info.route.as_ref().is_none_or(|route| route.destination != destination));
}

#[test]
fn failed_cover_search_waits_before_trying_again() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(2, 2);
    let threat = fixture.pos(1, 2);
    let mut waiting = info(CONTACT);
    waiting.mode = ActorMode::Evade { fleeing: false };
    waiting.evade_replan_remaining_secs = 0.4;
    waiting.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut waiting,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(waiting.route.is_none());

    let mut ready = info(CONTACT);
    ready.mode = ActorMode::Evade { fleeing: false };
    ready.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut ready,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(ready.route.is_some());
}

#[test]
fn no_cover_in_reach_sends_the_actor_fleeing_from_the_threat() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(4, 2);
    let threat = fixture.pos(1, 2);
    let mut info = info(CONTACT);
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut info,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert_eq!(info.mode, ActorMode::Evade { fleeing: true });
    let route = info.route.as_ref().expect("a flight leg");
    assert!(
        route.destination.horizontal_distance_sq(&threat) > actor_pos.horizontal_distance_sq(&threat),
        "ran toward the threat: {:?}",
        route.destination
    );
}

#[test]
fn a_flight_leg_is_kept_until_it_ends() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(4, 2);
    let threat = fixture.pos(1, 2);
    let leg = route_through(&[fixture.pos(5, 2), fixture.pos(6, 2)], &fixture);
    let mut info = info(CONTACT);
    info.mode = ActorMode::Evade { fleeing: true };
    info.route = Some(leg.clone());
    info.evade_replan_remaining_secs = 0.0;
    info.awareness.push(aware(7, threat, CharacterSupport::Ladder, true));

    enter_evade(
        &mut info,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert_eq!(info.route, Some(leg));

    info.route = None;
    info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;

    enter_evade(
        &mut info,
        &fixture.context(CONTACT, actor_pos),
        &mut StdRng::seed_from_u64(1),
    );

    assert!(info.route.is_some(), "no new leg on arrival");
}

#[test]
fn unarmed_players_are_not_evaded() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(CONTACT);
    info.awareness.push(aware(
        7,
        Position {
            y: 3.0,
            ..fixture.pos(4, 2)
        },
        CharacterSupport::Ladder,
        true,
    ));
    let context = BehaviorContext {
        players_armed: false,
        ..fixture.context(CONTACT, actor_pos)
    };

    decide_contact_actor(&mut info, &context, &mut StdRng::seed_from_u64(1));

    assert!(!matches!(info.mode, ActorMode::Evade { .. }), "evaded: {:?}", info.mode);
}
