use super::*;
use crate::actors::navigation::{GroundNavigation, GroundSearchOptions, GroundTask};

// Ticks at 30 Hz after which an airborne actor counts as falling.
fn grace_ticks() -> usize {
    (ACTOR_FALL_GRACE_SECS * 30.0).ceil() as usize
}

#[test]
fn a_falling_actor_keeps_its_mode_while_firing() {
    let (mut app, _, _) = actor_app(BEAM, 5000.0);
    let entity = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("actor missing")
        .entity;
    app.world_mut().entity_mut(entity).insert(CharacterSupport::Airborne);
    for _ in 0..grace_ticks() {
        step_tick(&mut app);
    }
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&ActorId(1))
        .expect("actor missing")
        .mode = ActorMode::Evade { fleeing: true };
    for _ in 0..3 {
        step_tick(&mut app);
    }
    let actors = app.world().resource::<ActorMap>();
    let info = actors.get(&ActorId(1)).expect("actor missing");
    assert!(matches!(info.mode, ActorMode::Evade { fleeing: true }));
    assert!(matches!(info.beam, BeamState::Firing { .. }));
}

#[test]
fn falling_actor_discards_routes_and_pending_searches_then_navigates_after_landing() {
    for supported in [CharacterSupport::Ground, CharacterSupport::Ladder] {
        let fixture = Fixture::new(CONTACT);
        let start = fixture.pos(1, 2);
        let target = fixture.pos(9, 2);
        let (mut app, _, _) = actor_app(CONTACT, 5000.0);
        app.world_mut().resource_mut::<ActorMap>().set_peaceful(true);
        let entity;
        {
            let mut actors = app.world_mut().resource_mut::<ActorMap>();
            let info = actors.get_mut(&ActorId(1)).expect("actor missing");
            entity = info.entity;
            info.ground.work = 1;
            info.ground.route(
                &GroundNavigation {
                    graphs: &fixture.graphs,
                    carriers: &fixture.carriers,
                    carrier: CarrierId::WORLD,
                    kind: CONTACT,
                    world: &fixture.collision_world,
                    physics: fixture.gameplay.expect_actor(CONTACT).physics(),
                    open: &[],
                },
                GroundTask::Return,
                start,
                target,
                |_, _| None,
                |_, _| true,
                GroundSearchOptions::default(),
            );
            assert!(info.ground.pending(GroundTask::Return));
            info.set_route(Some(route_through(&[target], &fixture)));
        }
        app.world_mut().entity_mut(entity).insert(CharacterSupport::Airborne);
        for tick in 1..=30 {
            app.world_mut().get_mut::<Position>(entity).expect("position missing").y = -(tick as f32);
            step_tick(&mut app);
            let actors = app.world().resource::<ActorMap>();
            let info = actors.get(&ActorId(1)).expect("actor missing");
            if tick < grace_ticks() {
                assert!(info.route.is_some(), "a brief hop dropped the ground route");
                continue;
            }
            assert!(info.route.is_none(), "falling actor retained a ground route");
            for task in [GroundTask::Roam, GroundTask::Return] {
                assert!(!info.ground.pending(task), "falling actor continued a ground search");
            }
        }
        app.world_mut().entity_mut(entity).insert((start, supported));
        for _ in 0..6 {
            step_tick(&mut app);
        }
        assert!(
            app.world()
                .resource::<ActorMap>()
                .get(&ActorId(1))
                .expect("actor missing")
                .route
                .is_some(),
            "supported actor did not resume navigation"
        );
    }
}

#[test]
fn falling_ground_actors_keep_beam_combat_active_without_ground_routes() {
    for kind in [BEAM, CONTACT_BEAM] {
        let (mut app, player, _) = actor_app(kind, 5000.0);
        let entity = app
            .world()
            .resource::<ActorMap>()
            .get(&ActorId(1))
            .expect("actor missing")
            .entity;
        app.world_mut().entity_mut(entity).insert(CharacterSupport::Airborne);
        for _ in 0..grace_ticks() {
            step_tick(&mut app);
        }
        assert!(app.world().get::<Health>(player).expect("health missing").0 < 5000.0);
        {
            let actors = app.world().resource::<ActorMap>();
            let info = actors.get(&ActorId(1)).expect("actor missing");
            assert_eq!(info.beam.target(), Some(PlayerId(7)));
            assert!(info.route.is_none());
        }
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .disconnect(&PlayerId(7), 2.0);
        step_tick(&mut app);
        let actors = app.world().resource::<ActorMap>();
        let info = actors.get(&ActorId(1)).expect("actor missing");
        assert!(matches!(info.beam, BeamState::Cooldown { .. }));
        assert!(info.awareness.is_empty());
        assert!(info.route.is_none());
    }
}
