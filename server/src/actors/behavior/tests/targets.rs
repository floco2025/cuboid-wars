use super::*;

#[test]
fn occluded_player_keeps_last_seen_state_without_refresh() {
    let fixture = Fixture::new(CONTACT);
    let actor_pos = fixture.pos(1, 2);
    let player = PlayerState {
        id: PlayerId(7),
        pos: fixture.pos(3, 2),
        carrier: CarrierId::WORLD,
        carrier_pos: fixture.pos(3, 2),
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
        carrier_pos: fixture.pos(4, 2),
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
            carrier: CarrierId::WORLD,
            carrier_pos: target,
            support: CharacterSupport::Ground,
        }],
        &fixture.collision_world,
    );
    assert!(info.awareness[0].visible);
    let mut context = fixture.context(BEAM, actor_pos);
    assert!(find_beam_target(&info, &context).is_none());
    assert!(matches!(info.beam, BeamState::Ready));
    let opened = [kind];
    context.open_fields = &opened;
    assert!(find_beam_target(&info, &context).is_some());
}

#[test]
fn immovable_actor_keeps_exposed_target_and_retargets_without_cooldown() {
    let fixture = Fixture::new(IMMOVABLE);
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
        tick_beam_state(&mut state, 1.0 / 30.0, context.kind_config, &[]);
        retarget_beam(&mut state, &context);
        assert_eq!(state.beam.target(), Some(PlayerId(7)));
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
    tick_beam_state(&mut state, cooldown + TICK_SECS, context.kind_config, &[]);
    decide_stationary_actor(&mut state, &context);
    assert_eq!(state.beam.target(), Some(PlayerId(7)));
}
