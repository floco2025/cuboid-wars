use super::*;
use common::protocol::BarrierId;

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
    assert_eq!(info.awareness[0].attack_anchor, Some(target));
}

#[test]
fn contact_actor_pursues_reachable_player_outside_home_region() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let target = fixture.pos(10, 2);
    assert!(
        !fixture
            .graph()
            .position_in_roam_region(&target, fixture.territories.get(0))
    );
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
    assert_eq!(route.destination, target);
    assert_eq!(route.waypoints.len(), 1);
    assert_eq!(route.next().map(|point| point.position), Some(target));
}

#[test]
fn jumping_target_keeps_its_last_ground_attack_anchor() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let anchor = fixture.pos(4, 2);
    let mut info = info(CONTACT);
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: anchor,
    };
    let mut target = aware(7, Position { y: 2.0, ..anchor }, CharacterSupport::Airborne, true);
    target.attack_anchor = Some(anchor);
    info.awareness.push(target);
    let mut rng = StdRng::seed_from_u64(1);

    decide_contact_actor(&mut info, &fixture.context(CONTACT, actor_pos), &mut rng);

    assert!(matches!(info.mode, ActorMode::Engage { target: PlayerId(7), target_pos } if target_pos == anchor));
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
        fixture.server.actors.settings.threat_memory_secs,
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
    let target = fixture.pos(3, 2);
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
    let kind = BarrierKindId(0);
    let x = (actor_pos.x + target.x) / 2.0;
    let layout = MapLayout {
        barriers: vec![Barrier {
            id: Default::default(),

            switch: None,
            switch_inverted: false,

            x1: x,
            x2: x,
            z1: actor_pos.z - 4.0,
            z2: actor_pos.z + 4.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            kind,
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
    let opened = [BarrierId(u32::from(kind.0))];
    context.open_barriers = &opened;
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
    let kind = BarrierKindId(0);
    let x = (origin.x + target.x) / 2.0;
    fixture.collision_world = CollisionWorld::from_map_layout(&MapLayout {
        barriers: vec![Barrier {
            id: Default::default(),

            switch: None,
            switch_inverted: false,

            x1: x,
            x2: x,
            z1: origin.z - 4.0,
            z2: origin.z + 4.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let mut state = info(IMMOVABLE);
    state.awareness.push(aware(7, target, CharacterSupport::Ground, true));
    let mut context = fixture.context(IMMOVABLE, origin);
    let opened = [BarrierId(u32::from(kind.0))];
    context.open_barriers = &opened;
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
    context.open_barriers = &[];
    retarget_beam(&mut state, &context);
    assert!(matches!(state.beam, BeamState::Cooldown { .. }));
    context.open_barriers = &opened;
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
