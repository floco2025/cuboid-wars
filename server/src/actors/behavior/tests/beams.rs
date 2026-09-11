use super::*;

fn actor_app(kind: &str, health: f32) -> (App, Entity, UnboundedReceiver<ServerToClient>) {
    let fixture = Fixture::new(kind);
    let origin = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let character = ActorCharacter(fixture.gameplay.expect_actor(kind).clone());
    let mut app = App::new();
    app.insert_resource(fixture.graphs)
        .insert_resource(fixture.territories)
        .insert_resource(fixture.carriers)
        .insert_resource(fixture.collision_world)
        .insert_resource(fixture.gameplay)
        .insert_resource(fixture.server)
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<MapItems>()
        .init_resource::<PlateState>()
        .init_resource::<ServerTick>()
        .init_resource::<Time>()
        .init_resource::<PendingExplosions>()
        .insert_resource(Invincibility(false))
        .add_systems(Update, (actors_behavior_system, actors_beam_damage_system).chain());
    let actor = app.world_mut().spawn((ActorId(1), ActorMarker, origin, character)).id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(ActorId(1), ActorInfo::new(actor, 0, kind.into(), CarrierId::WORLD));
    let player = app
        .world_mut()
        .spawn((PlayerMarker, PlayerId(7), target, Health(health)))
        .id();
    let (sender, receiver) = unbounded_channel();
    let mut player_info = PlayerInfo::new(player, sender);
    player_info.connection.logged_in = true;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(PlayerId(7), player_info);
    (app, player, receiver)
}

fn step_tick(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(1.0 / 30.0));
    app.world_mut().resource_mut::<ServerTick>().0 += 1;
    app.update();
}

#[test]
fn immovable_actor_fires_over_cover_below_its_gun_despite_its_lower_body_center() {
    let (mut app, player, _) = actor_app(IMMOVABLE, 5000.0);
    let actor = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("actor missing")
        .entity;
    let actor_pos = *app.world().get::<Position>(actor).expect("actor position missing");
    let player_pos = *app.world().get::<Position>(player).expect("player position missing");
    let gameplay = app.world().resource::<GameplayConfig>();
    let immovable = gameplay.expect_actor(IMMOVABLE);
    let target_y = gameplay.player.physics().hitbox_center_y(player_pos.y);
    let body_ray_y = (immovable.physics().hitbox_center_y(actor_pos.y) + target_y) / 2.0;
    let gun_ray_y = (actor_pos.y + immovable.beam_origin_y_offset() + target_y) / 2.0;
    let cover_height = (body_ray_y + gun_ray_y) / 2.0 - actor_pos.y;
    let wall_x = (actor_pos.x + player_pos.x) / 2.0;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: wall_x,
            z1: actor_pos.z - 2.0,
            x2: wall_x,
            z2: actor_pos.z + 2.0,
            width: 0.1,
            y: actor_pos.y,
            height: cover_height,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout));
    step_tick(&mut app);
    assert!(app.world().get::<Health>(player).expect("player health missing").0 < 5000.0);
}

#[test]
fn peace_stops_attacks_and_targeting_until_disabled_for_every_actor_kind() {
    for kind in KINDS {
        let (mut app, player, _) = actor_app(kind, 5000.0);
        step_tick(&mut app);
        let health = app.world().get::<Health>(player).expect("player health missing").0;
        app.world_mut().resource_mut::<ActorMap>().set_peaceful(true);
        for _ in 0..30 {
            step_tick(&mut app);
            let info = app
                .world()
                .resource::<ActorMap>()
                .get(&ActorId(1))
                .expect("actor missing");
            assert!(info.awareness.is_empty(), "{kind} noticed a player during peace");
            assert!(info.beam.target().is_none(), "{kind} fired during peace");
            assert!(!matches!(info.mode, ActorMode::Engage { .. } | ActorMode::Evade { .. }));
            assert_eq!(
                app.world().get::<Health>(player).expect("player health missing").0,
                health
            );
        }
        app.world_mut().resource_mut::<ActorMap>().set_peaceful(false);
        step_tick(&mut app);
        let info = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing");
        assert!(
            !info.awareness.is_empty(),
            "{kind} failed to notice players after peace"
        );
        if [IMMOVABLE, BEAM, CONTACT_BEAM].contains(&kind) {
            assert!(info.beam.target().is_some(), "{kind} failed to resume firing");
        }
    }
}

#[test]
fn immovable_actor_holds_long_burst_and_stops_when_player_disconnects() {
    let (mut app, player, mut receiver) = actor_app(IMMOVABLE, 5000.0);
    for _ in 0..120 {
        step_tick(&mut app);
    }
    let health = app.world().get::<Health>(player).expect("player health missing").0;
    assert!((health - 3000.0).abs() < 0.1);
    let actor = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("actor missing");
    assert!(actor.route.is_none());
    assert_eq!(actor.beam.target(), Some(PlayerId(7)));
    let mut targets = Vec::new();
    while let Ok(ServerToClient::Send(message)) = receiver.try_recv() {
        if let ServerMessage::ActorBeam(cue) = message {
            targets.push(cue.beam.map(|beam| beam.target));
        }
    }
    assert_eq!(targets, vec![Some(PlayerId(7))]);
    let cooldown = app
        .world()
        .resource::<ServerGameplayConfig>()
        .expect_actor(IMMOVABLE)
        .attack
        .beam()
        .expect("actor fires no beam")
        .cooldown_secs;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(7), 2.0);
    step_tick(&mut app);
    assert_eq!(
        app.world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .beam,
        BeamState::Cooldown {
            remaining_secs: cooldown
        }
    );
    assert_eq!(
        app.world().get::<Health>(player).expect("player health missing").0,
        health
    );
}

#[test]
fn immovable_actor_repeats_bursts_with_a_damage_free_cooldown_and_transition_cues() {
    let (mut app, player, mut receiver) = actor_app(IMMOVABLE, 50000.0);
    let attack = app
        .world()
        .resource::<ServerGameplayConfig>()
        .expect_actor(IMMOVABLE)
        .attack
        .beam()
        .expect("beam config missing");
    let total_ticks = ((attack.duration_secs + attack.cooldown_secs) / TICK_SECS).ceil() as u32 + 5;
    let mut previous_health = 50000.0;
    let mut cooldown_ticks = 0;
    for _ in 0..total_ticks {
        step_tick(&mut app);
        let health = app.world().get::<Health>(player).expect("player health missing").0;
        let actor = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing");
        if actor.beam.target().is_none() {
            cooldown_ticks += 1;
            assert_eq!(health, previous_health);
        } else {
            assert!(health < previous_health);
        }
        previous_health = health;
    }
    let expected_cooldown_ticks = (attack.cooldown_secs / TICK_SECS).ceil() as u32;
    assert!((expected_cooldown_ticks..=expected_cooldown_ticks + 1).contains(&cooldown_ticks));
    let mut cues = Vec::new();
    while let Ok(ServerToClient::Send(message)) = receiver.try_recv() {
        if let ServerMessage::ActorBeam(cue) = message {
            cues.push(cue);
        }
    }
    assert_eq!(cues.len(), 3);
    let first = cues[0].beam.expect("first burst missing");
    assert!(cues[1].beam.is_none());
    let second = cues[2].beam.expect("second burst missing");
    assert_eq!(first.target, second.target);
    assert_ne!(first.started_tick, second.started_tick);
    let actual_duration = (cues[1].tick - cues[0].tick) as f32 * TICK_SECS;
    assert!((actual_duration - attack.duration_secs).abs() <= TICK_SECS);
}

#[test]
fn active_beams_retarget_disconnected_players_before_the_next_navigation_decision() {
    for kind in [IMMOVABLE, BEAM, CONTACT_BEAM] {
        let (mut app, player, _) = actor_app(kind, 5000.0);
        step_tick(&mut app);
        let first = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .beam
            .snapshot()
            .expect("first burst missing");
        let pos = *app.world().get::<Position>(player).expect("player position missing");
        let next = app
            .world_mut()
            .spawn((PlayerMarker, PlayerId(8), pos, Health(5000.0)))
            .id();
        let (sender, _receiver) = unbounded_channel();
        let mut info = PlayerInfo::new(next, sender);
        info.connection.logged_in = true;
        app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(8), info);
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .disconnect(&PlayerId(7), 2.0);
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&ActorId(1))
            .expect("actor missing")
            .decision_timer = 1.0;
        step_tick(&mut app);
        let after = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .beam
            .snapshot()
            .expect("burst ended during retarget");
        assert_eq!(after.target, PlayerId(8), "{kind}");
        assert_eq!(after.started_tick, first.started_tick, "{kind}");
        assert!(after.remaining_secs < first.remaining_secs, "{kind}");
    }
}
