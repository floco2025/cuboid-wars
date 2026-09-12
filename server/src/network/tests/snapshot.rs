use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    config::{ActorLocomotion, NetworkConfig},
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::*,
};
use crossbeam_channel::unbounded;

use super::{broadcast::snapshot_actors, snapshot::network_broadcast_actor_moves_system};
use crate::{
    actors::{
        ActorCharacter, ActorInfo, ActorMap, ActorMotionQuery, ActorStateQuery,
        test_kinds::{self, CONTACT},
    },
    players::{PlayerInfo, PlayerMap},
};

#[test]
fn actor_updates_repeat_full_state_at_the_configured_rate_in_the_carrier_frame() {
    for hz in [1, 7, 10, 30] {
        let layout = MapLayout {
            carriers: vec![Carrier {
                switch_inverted: false,

                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::default(),
                to: Position {
                    x: 12.0,
                    y: 4.0,
                    z: 0.0,
                },
                travel_ticks: 30,
                pause_ticks: 5,
                phase_ticks: 0,
                switch: None,
            }],
            ..default()
        };
        let mut app = App::new();
        app.insert_resource(NetworkConfig {
            server_hz: 30,
            update_hz: hz,
            snapshot_hz: 4,
        })
        .insert_resource(Carriers::from_layout(&layout))
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<ServerTick>()
        .add_systems(Update, network_broadcast_actor_moves_system);
        let (sender, receiver) = unbounded();
        let player_entity = app.world_mut().spawn_empty().id();
        let mut player = PlayerInfo::new(player_entity, sender);
        player.connection.logged_in = true;
        app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(1), player);
        let id = ActorId(2);
        let entity = app
            .world_mut()
            .spawn((
                id,
                ActorMarker,
                ActorCharacter(test_kinds::kind(CONTACT).character),
                Position::default(),
                ActorMoveIntent::Idle,
                FaceYaw(1.25),
                CharacterVerticalVelocity(0.0),
                CharacterSupport::Ground,
                Health(100.0),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ActorMap>()
            .insert(id, ActorInfo::new(entity, 0, "actor".into(), CarrierId(1)));
        let local = Position {
            x: 2.0,
            y: 0.5,
            z: -1.0,
        };
        let mut updates = 0;
        for tick in 1..=60 {
            app.world_mut().resource_mut::<ServerTick>().0 = tick;
            app.world_mut()
                .resource_mut::<Carriers>()
                .advance(tick, &PlateState::default());
            let pos = app
                .world()
                .resource::<Carriers>()
                .pose(CarrierId(1))
                .transform_position(&local);
            app.world_mut().entity_mut(entity).insert(pos);
            app.update();
            while let Ok(message) = receiver.try_recv() {
                let ServerMessage::ActorMoves(message) = message else {
                    panic!("unexpected message");
                };
                updates += 1;
                assert_eq!(message.tick, tick);
                assert_eq!(message.moves.len(), 1);
                let entry = message.moves[0];
                assert_eq!(entry.id, id);
                assert_eq!(entry.movement.carrier, CarrierId(1));
                assert!((Vec3::from(entry.movement.pos) - Vec3::from(local)).length() < 1e-5);
                assert_eq!(entry.movement.face_yaw, 1.25);
                assert_eq!(entry.movement.support, CharacterSupport::Ground);
                assert_eq!(entry.movement.move_intent, ActorMoveIntent::Idle);
                let mut state = SystemState::<(ActorStateQuery, ActorMotionQuery)>::new(app.world_mut());
                let (actors, motions) = state.get(app.world()).expect("actor queries are invalid");
                let snapshot = snapshot_actors(
                    app.world().resource::<ActorMap>(),
                    &actors,
                    &motions,
                    app.world().resource::<Carriers>(),
                );
                assert_eq!(snapshot[0].1.movement, entry.movement);
            }
        }
        assert_eq!(updates, hz * 2);
        app.world_mut().resource_mut::<ActorMap>().remove(&id);
        for _ in 0..30 {
            app.update();
        }
        assert!(receiver.try_recv().is_err(), "removed actor was still broadcast");
    }
}

#[test]
fn flying_actor_snapshots_use_world_positions_independent_of_spawn_carrier() {
    let mut app = App::new();
    let position = Position {
        x: 20.0,
        y: -90.0,
        z: 10.0,
    };
    let intent = ActorMoveIntent::Flying {
        velocity: [1.0, 2.0, 3.0],
    };
    let mut character = test_kinds::kind(CONTACT).character;
    character.locomotion = ActorLocomotion::Flying;
    let entity = app
        .world_mut()
        .spawn((
            ActorMarker,
            ActorCharacter(character),
            position,
            intent,
            FaceYaw(1.0),
            Health(50.0),
            CharacterVerticalVelocity(2.0),
            CharacterSupport::Airborne,
        ))
        .id();
    let mut actors = ActorMap::default();
    let mut info = ActorInfo::new(entity, 0, "flyer".into(), CarrierId(1));
    info.flight = Some(Default::default());
    actors.insert(ActorId(1), info);
    let mut query = SystemState::<(ActorStateQuery, ActorMotionQuery)>::new(app.world_mut());
    let (bodies, motions) = query.get(app.world()).expect("actor snapshot query invalid");
    let snapshot = snapshot_actors(&actors, &bodies, &motions, &Carriers::default());
    let state = snapshot[0].1.movement;
    assert_eq!(state.carrier, CarrierId::WORLD);
    assert_eq!(state.pos, position);
    assert_eq!(state.move_intent, intent);
}
