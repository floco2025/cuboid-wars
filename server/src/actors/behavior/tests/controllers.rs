use super::*;
use crate::test_geometry::WALL_THICKNESS;
use common::protocol::FieldId;

#[test]
fn contact_actor_engages_reachable_ground_player() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info(CONTACT);
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
}

#[test]
fn contact_actor_pursues_reachable_player_outside_home_region() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(10, 2);
    assert!(!fixture.territories.get(0).contains_position(target.into()));
    let mut info = info(CONTACT);
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    let route = info.route.as_ref().expect("engagement should install a route");
    assert!(super::super::geometry::attack_position(
        route.destination,
        target,
        &(&fixture.context(CONTACT, actor_pos)).into()
    ));
}

#[test]
fn jumping_target_is_pursued_to_a_reachable_attack_position() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let anchor = fixture.pos(4, 2);
    let mut info = info(CONTACT);
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: anchor,
    };
    let target = aware(7, Position { y: 0.8, ..anchor }, CharacterSupport::Airborne, true);
    info.awareness.push(target);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Engage { target: PlayerId(7), target_pos } if target_pos == target.pos));
    let route = info.route.as_ref().expect("reachable jumping target has no route");
    assert!(super::super::geometry::attack_position(
        route.destination,
        target.pos,
        &(&fixture.context(CONTACT, actor_pos)).into()
    ));
}

#[test]
fn ladder_target_makes_contact_actor_evade() {
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
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
}

#[test]
fn reachable_player_has_priority_over_fleeing_from_another_player() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(CONTACT);
    info.awareness.push(aware(
        7,
        Position {
            y: 3.0,
            ..fixture.pos(2, 2)
        },
        CharacterSupport::Ladder,
        true,
    ));
    info.awareness
        .push(aware(8, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(8),
            ..
        }
    ));
}

#[test]
fn ready_beam_actor_fires_at_visible_player_in_range() {
    let fixture = Fixture::new(BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info(BEAM);
    info.awareness.push(aware(7, target, CharacterSupport::Ladder, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_beam_actor(&mut info, &fixture.context(BEAM, actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Firing { .. }));
    assert_eq!(
        outcome,
        Some(ActorBeam {
            target: PlayerId(7),
            started_tick: 0,
            remaining_secs: fixture
                .server
                .expect_actor(BEAM)
                .attack
                .beam()
                .expect("beam config")
                .duration_secs,
        })
    );
}

#[test]
fn beam_actor_acquires_visible_cross_level_player_in_beam_range() {
    let fixture = Fixture::with_levels(BEAM, 2);
    let actor_pos = fixture.pos(1, 2);
    let target = Position {
        y: LEVEL_HEIGHT,
        ..actor_pos
    };
    let kind = fixture.server.expect_actor(BEAM);
    let beam_actor = fixture.gameplay.expect_actor(BEAM);
    assert!(
        fixture
            .graph()
            .engagement_route(
                &[],
                &actor_pos,
                &target,
                beam_actor.physics().movement_collider.radius(),
                beam_actor.physics().movement_collider.radius(),
            )
            .is_none()
    );
    assert!(actor_pos.distance_sq(&target) <= kind.attack.beam().expect("beam config").range.powi(2));
    let mut info = info(BEAM);
    update_awareness(
        &mut info,
        actor_pos,
        fixture.gameplay.expect_actor(BEAM).eye_height(),
        kind.vision_range,
        kind.threat_memory_secs,
        fixture.gameplay.player.physics(),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
        &fixture.collision_world,
    );
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_beam_actor(&mut info, &fixture.context(BEAM, actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Firing { .. }));
    assert!(matches!(
        outcome,
        Some(ActorBeam {
            target: PlayerId(7),
            ..
        })
    ));
}

#[test]
fn cooling_beam_actor_evades_instead_of_approaching() {
    let fixture = Fixture::new(BEAM);
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(BEAM);
    info.beam = BeamState::Cooldown { remaining_secs: 4.0 };
    info.awareness
        .push(aware(7, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_beam_actor(&mut info, &fixture.context(BEAM, actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
}

#[test]
fn completed_beam_enters_cooldown_and_evade() {
    let fixture = Fixture::new(BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let mut info = info(BEAM);
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 0.05,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    tick_runtime_state(
        &mut info,
        actor_pos,
        0.1,
        fixture.server.expect_actor(BEAM),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
    );

    assert_eq!(
        info.beam,
        BeamState::Cooldown {
            remaining_secs: fixture
                .server
                .expect_actor(BEAM)
                .attack
                .beam()
                .expect("beam config")
                .cooldown_secs,
        }
    );
    assert_eq!(info.decision_timer, 0.0);

    decide_beam_actor(&mut info, &fixture.context(BEAM, actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Evade { .. }));
}

#[test]
fn ready_contact_beam_actor_fires_and_keeps_its_engagement_route() {
    let fixture = Fixture::new(CONTACT_BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info(CONTACT_BEAM);
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context(CONTACT_BEAM, actor_pos), &mut rng);

    assert!(matches!(
        info.beam,
        BeamState::Firing {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
    assert_eq!(outcome.map(|started| started.target), Some(PlayerId(7)));
}

#[test]
fn cooling_contact_beam_actor_keeps_engaging_instead_of_evading() {
    let fixture = Fixture::new(CONTACT_BEAM);
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(CONTACT_BEAM);
    info.beam = BeamState::Cooldown { remaining_secs: 4.0 };
    info.awareness
        .push(aware(7, fixture.pos(4, 2), CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = decide_contact_beam_actor(&mut info, &fixture.context(CONTACT_BEAM, actor_pos), &mut rng);

    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
    assert_eq!(outcome, None);
}

#[test]
fn firing_contact_beam_actor_without_reachable_target_holds_facing_beam_target() {
    let fixture = Fixture::new(CONTACT_BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = Position {
        y: 20.0,
        ..fixture.pos(3, 2)
    };
    let mut info = info(CONTACT_BEAM);
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 1.0,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ladder, true));
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_beam_actor(&mut info, &fixture.context(CONTACT_BEAM, actor_pos), &mut rng);

    assert_eq!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            target_pos: target,
        }
    );
    assert!(info.route.is_none());
}

#[test]
fn contact_beam_actor_burst_end_enters_cooldown_without_evading() {
    let fixture = Fixture::new(CONTACT_BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(4, 2);
    let mut info = info(CONTACT_BEAM);
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 0.05,
    };
    info.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut rng = StdRng::seed_from_u64(1);

    tick_runtime_state(
        &mut info,
        actor_pos,
        0.1,
        fixture.server.expect_actor(CONTACT_BEAM),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
    );
    decide_contact_beam_actor(&mut info, &fixture.context(CONTACT_BEAM, actor_pos), &mut rng);

    assert!(matches!(info.beam, BeamState::Cooldown { .. }));
    assert!(matches!(
        info.mode,
        ActorMode::Engage {
            target: PlayerId(7),
            ..
        }
    ));
    assert!(info.route.is_some());
}

#[test]
fn actor_outside_roam_region_routes_home() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(10, 2);
    let mut info = info(CONTACT);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::ReturnHome);
    assert!(info.route.is_some());
}

#[test]
fn actor_inside_roam_region_chooses_a_roam_route() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(CONTACT);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert_eq!(info.mode, ActorMode::Roam);
    assert!(info.route.is_some());
}

#[test]
fn occluded_player_keeps_last_seen_state_without_refresh() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let player = PlayerState {
        id: PlayerId(7),
        pos: fixture.pos(3, 2),
        support: CharacterSupport::Ground,
    };
    let mut info = info(CONTACT);
    let actor = fixture.gameplay.expect_actor(CONTACT);
    update_awareness(
        &mut info,
        actor_pos,
        actor.eye_height(),
        60.0,
        10.0,
        fixture.gameplay.player.physics(),
        &[player],
        &fixture.collision_world,
    );
    assert_eq!(info.awareness.len(), 1);
    info.awareness[0].forget_remaining_secs = 4.0;

    let wall_x = (actor_pos.x + player.pos.x) / 2.0;
    let blocked_world = CollisionWorld::from_map_layout(&MapLayout {
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
    let moved_player = PlayerState {
        pos: fixture.pos(4, 2),
        support: CharacterSupport::Ladder,
        ..player
    };
    update_awareness(
        &mut info,
        actor_pos,
        actor.eye_height(),
        60.0,
        10.0,
        fixture.gameplay.player.physics(),
        &[moved_player],
        &blocked_world,
    );

    assert_eq!(info.awareness.len(), 1);
    assert!(!info.awareness[0].visible);
    assert_eq!(info.awareness[0].pos, player.pos);
    assert_eq!(info.awareness[0].support, CharacterSupport::Ground);
    assert_eq!(info.awareness[0].forget_remaining_secs, 4.0);
}

#[test]
fn beam_actor_sees_a_player_through_a_barrier_but_waits_for_a_clear_attack() {
    let mut fixture = Fixture::new(BEAM);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let kind = FieldId(0);
    let x = (actor_pos.x + target.x) / 2.0;
    let layout = MapLayout {
        barriers: vec![Barrier {
            x1: x,
            x2: x,
            z1: actor_pos.z - 4.0,
            z2: actor_pos.z + 4.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            field: kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    fixture.collision_world = CollisionWorld::from_map_layout(&layout);
    let mut info = info(BEAM);
    update_awareness(
        &mut info,
        actor_pos,
        fixture.gameplay.expect_actor(BEAM).eye_height(),
        60.0,
        1.0,
        fixture.gameplay.player.physics(),
        &[PlayerState {
            id: PlayerId(7),
            pos: target,
            support: CharacterSupport::Ground,
        }],
        &fixture.collision_world,
    );
    assert!(info.awareness[0].visible);
    let mut rng = StdRng::seed_from_u64(1);
    let mut context = fixture.context(BEAM, actor_pos);
    assert!(decide_beam_actor(&mut info, &context, &mut rng).is_none());
    assert!(matches!(info.beam, BeamState::Ready));
    let opened = [kind];
    context.open_fields = &opened;
    assert!(decide_beam_actor(&mut info, &context, &mut rng).is_some());
}

#[test]
fn immovable_actor_keeps_exposed_target_and_retargets_without_cooldown() {
    let fixture = Fixture::with_levels(IMMOVABLE, 2);
    let actor_pos = fixture.pos(1, 2);
    let mut state = info(IMMOVABLE);
    let mut above = fixture.pos(3, 2);
    above.y = LEVEL_HEIGHT;
    state.awareness = vec![
        aware(7, above, CharacterSupport::Ground, true),
        aware(8, fixture.pos(2, 2), CharacterSupport::Ground, true),
    ];
    state.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 14.0,
    };
    let context = fixture.context(IMMOVABLE, actor_pos);
    for _ in 0..120 {
        tick_runtime_state(&mut state, actor_pos, 1.0 / 30.0, context.kind_config, &[]);
        retarget_beam(&mut state, &context);
        assert_eq!(state.beam.target(), Some(PlayerId(7)));
        assert!(state.route.is_none());
    }
    let before = state.beam.snapshot().expect("burst missing");
    state.awareness[0].visible = false;
    retarget_beam(&mut state, &context);
    let after = state.beam.snapshot().expect("burst ended during retarget");
    assert_eq!(after.target, PlayerId(8));
    assert_eq!(after.started_tick, before.started_tick);
    assert_eq!(after.remaining_secs, before.remaining_secs);
    state.awareness[1].pos.x += 100.0;
    retarget_beam(&mut state, &context);
    assert!(matches!(state.beam, BeamState::Cooldown { .. }));
    state.awareness.clear();
    retarget_beam(&mut state, &context);
    assert!(state.route.is_none());
}

#[test]
fn closing_a_barrier_immediately_stops_an_immovable_actor() {
    let mut fixture = Fixture::new(IMMOVABLE);
    let origin = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let kind = FieldId(0);
    let x = (origin.x + target.x) / 2.0;
    fixture.collision_world = CollisionWorld::from_map_layout(&MapLayout {
        barriers: vec![Barrier {
            x1: x,
            x2: x,
            z1: origin.z - 4.0,
            z2: origin.z + 4.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            field: kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let mut state = info(IMMOVABLE);
    state.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut context = fixture.context(IMMOVABLE, origin);
    let opened = [kind];
    context.open_fields = &opened;
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
    context.open_fields = &[];
    retarget_beam(&mut state, &context);
    assert!(matches!(state.beam, BeamState::Cooldown { .. }));
    context.open_fields = &opened;
    assert!(decide_stationary_actor(&mut state, &context).is_none());
    let cooldown = context
        .kind_config
        .attack
        .beam()
        .expect("actor fires no beam")
        .cooldown_secs;
    tick_runtime_state(&mut state, origin, cooldown + TICK_SECS, context.kind_config, &[]);
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
}

// Pursuit only ever installs a route the search accepted, so both tests
// call the engagement until it has one or gives the target up.
fn pursue_until_settled(
    info: &mut ActorInfo,
    fixture: &Fixture,
    actor_pos: Position,
    target: Position,
    max_calls: usize,
) -> (bool, usize) {
    for call in 1..=max_calls {
        info.ground.work = 256;
        let engaged = keep_or_install_engagement_route(info, &fixture.context(CONTACT, actor_pos), PlayerId(7), target);
        if !engaged || info.route.is_some() {
            return (engaged, call);
        }
    }
    (true, max_calls)
}

#[test]
fn contact_actor_reaches_touching_distance_of_a_player_against_a_wall() {
    let geometry = geometry(12, 5);
    let wall_x = geometry.cell_to_world_x(5);
    let layout = MapLayout {
        walls: vec![Wall {
            x1: wall_x,
            z1: geometry.cell_to_world_z(0),
            x2: wall_x,
            z2: geometry.cell_to_world_z(5),
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let fixture = Fixture::with_world(CONTACT, CollisionWorld::from_map_layout(&layout));
    let player_radius = fixture.gameplay.player.physics().movement_collider.radius();
    // As close to the wall as the player's own body allows.
    let target = Position {
        x: wall_x - WALL_THICKNESS / 2.0 - player_radius - 0.02,
        y: 0.0,
        z: geometry.cell_center_z(2),
    };
    let actor_pos = fixture.pos(1, 2);
    let mut info = info(CONTACT);

    let (engaged, _) = pursue_until_settled(&mut info, &fixture, actor_pos, target, 20);

    assert!(engaged, "a player against a wall is still attackable");
    let route = info.route.as_ref().expect("engagement installs a route");
    let context = fixture.context(CONTACT, actor_pos);
    assert!(super::super::geometry::attack_position(
        route.destination,
        target,
        &(&context).into()
    ));
    assert!(
        !fixture
            .collision_world
            .character_overlaps_wall(&route.destination, context.actor_physics),
        "the attack position keeps the body out of the wall"
    );
    assert!(
        route.destination.x < target.x,
        "the actor stops on its own side of the player"
    );
}

#[test]
fn a_target_no_node_can_attack_is_given_up_after_the_search_limit() {
    let geometry = geometry(12, 5);
    let layout = MapLayout {
        grounds: Some(common::map::Grounds::new(
            [(
                -geometry.width() / 2.0,
                geometry.width() / 2.0,
                -geometry.depth() / 2.0,
                geometry.depth() / 2.0,
            )],
            0.0,
            common::map::GroundsSettings { level: 0 },
        )),
        ..Default::default()
    };
    let mut fixture = Fixture::with_world(CONTACT, CollisionWorld::from_map_layout(&layout));
    fixture.graphs.add_grounds(&layout);
    let actor_pos = fixture.pos(1, 2);
    let target = Position {
        y: -40.0,
        ..fixture.pos(4, 2)
    };
    let mut info = info(CONTACT);

    let (engaged, calls) = pursue_until_settled(&mut info, &fixture, actor_pos, target, 200);

    assert!(!engaged, "no attack position exists for a target far below the floor");
    assert!(info.route.is_none());
    // The grounds continue the grid for hundreds of thousands of cells; a
    // bounded search settles within a handful of ticks' work.
    assert!(calls > 1, "the search takes more than one tick's work");
    assert!(calls <= 4096 / 256 + 2, "the search gave up after {calls} calls");
}
