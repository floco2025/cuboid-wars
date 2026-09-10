use super::{
    AirGraph, MissileFlight, MissileInfo, MissileMap, MissileVelocity, OwnedMissile, RemoteMissileMotion,
    missiles_movement_system,
};
use crate::{
    actors::ActorMap,
    characters::PreviousTickPosition,
    network::{ClientToServer, ClientToServerChannel},
    players::{PlayerInfo, PlayerMap},
    test_fixtures,
    vfx::BlastRadii,
};
use bevy::prelude::*;
use common::{config::NetworkConfig, constants::TICK_SECS, map::Carriers, physics::CollisionWorld, protocol::*};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

fn app(hz: u32) -> (App, UnboundedReceiver<ClientToServer>) {
    let mut app = App::new();
    let (sender, receiver) = unbounded_channel();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(TICK_SECS));
    let layout = MapLayout::default();
    app.insert_resource(time)
        .insert_resource(NetworkConfig {
            server_hz: 30,
            update_hz: hz,
            snapshot_hz: 4,
        })
        .insert_resource(test_fixtures::gameplay_config())
        .insert_resource(test_fixtures::map_settings())
        .insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()))
        .insert_resource(AirGraph::new(&[], test_fixtures::sizes()))
        .insert_resource(BlastRadii {
            player: 5.0,
            missile: 5.0,
            actors: HashMap::new(),
        })
        .insert_resource(ClientToServerChannel::new(sender))
        .init_resource::<Carriers>()
        .init_resource::<MissileMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<PlateState>()
        .add_systems(Update, missiles_movement_system);
    (app, receiver)
}

fn missile(app: &mut App, id: MissileId, owned: bool, speed: f32, lifetime: f32) -> Entity {
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
    let velocity = Vec3::X * speed;
    let entity = app
        .world_mut()
        .spawn((
            MissileMarker,
            id,
            pos,
            PreviousTickPosition(pos),
            MissileVelocity(velocity),
        ))
        .id();
    let remote = if owned {
        app.world_mut().entity_mut(entity).insert(OwnedMissile {
            flight: MissileFlight::new(PlayerId(1), None, 0.0, lifetime),
            seq: 0,
        });
        None
    } else {
        Some(RemoteMissileMotion::new(
            0,
            MissileMovementState::from_velocity(pos, velocity),
            6.0,
        ))
    };
    app.world_mut().resource_mut::<MissileMap>().entries.insert(
        id,
        MissileInfo {
            entity,
            shooter: PlayerId(if owned { 1 } else { 2 }),
            born_tick: 0,
            remote,
        },
    );
    entity
}

#[test]
fn only_shooters_flights_simulate_and_report_at_the_configured_rate_without_a_living_shooter() {
    for hz in [7, 10, 30] {
        let (mut app, mut receiver) = app(hz);
        let local = missile(&mut app, MissileId(1), true, 20.0, 10.0);
        let remote = missile(&mut app, MissileId(2), false, 100.0, 10.0);
        for _ in 0..30 {
            app.update();
        }
        assert!((app.world().get::<Position>(local).expect("owned missile missing").x - 20.0).abs() < 1e-4);
        assert_eq!(
            app.world().get::<Position>(remote).expect("observer missile missing").x,
            0.0
        );
        let messages: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        assert_eq!(messages.len(), hz as usize);
        let mut last = 0;
        for message in messages {
            let ClientToServer::Send(ClientMessage::MissileMoves(message)) = message else {
                panic!("unexpected flight report");
            };
            assert_eq!(message.moves.len(), 1);
            let update = &message.moves[0];
            assert_eq!(update.id, MissileId(1));
            assert!(sequence_is_newer(update.seq, last));
            assert!((update.movement.pos.x - update.seq as f32 * 20.0 * TICK_SECS).abs() < 1e-4);
            last = update.seq;
        }
        assert!(last > 30 - 30_u32.div_ceil(hz) && last <= 30);
    }
}

#[test]
fn a_fast_missile_reports_the_swept_hit_and_victim_generation_once() {
    let (mut app, mut receiver) = app(10);
    let id = PlayerId(2);
    let pos = Position { x: 5.0, y: 0.0, z: 0.0 };
    let target = app.world_mut().spawn((PlayerMarker, id, pos, FaceYaw(0.0))).id();
    let mut player = Player::new("Player".into(), pos, PlayerMoveIntent::Idle, 0.0, 0, Health(100.0));
    player.generation = PlayerGeneration(4);
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::from_snapshot(target, &player, 0));
    let entity = missile(&mut app, MissileId(3), true, 300.0, 10.0);
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    let ClientToServer::Send(message) = receiver.try_recv().expect("detonation report missing") else {
        panic!("unexpected close");
    };
    assert_eq!(message.lane(), Lane::Reliable);
    let ClientMessage::MissileDetonated(report) = message else {
        panic!("expected detonation");
    };
    assert!(
        report.pos.x > 3.0 && report.pos.x < 5.0,
        "missile tunneled through its target"
    );
    assert_eq!(report.hits.len(), 1);
    assert_eq!(
        report.hits[0].target,
        HitTarget::Player {
            id,
            generation: PlayerGeneration(4)
        }
    );
    assert_eq!(report.hits[0].falloff, 1.0);
    for _ in 0..5 {
        app.update();
    }
    assert!(receiver.try_recv().is_err(), "detonation was sent more than once");
}

#[test]
fn lifetime_detonation_needs_no_target_or_movement_packet() {
    let (mut app, mut receiver) = app(1);
    let entity = missile(&mut app, MissileId(1), true, 20.0, TICK_SECS * 1.5);
    app.update();
    while receiver.try_recv().is_ok() {}
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    let ClientToServer::Send(ClientMessage::MissileDetonated(report)) =
        receiver.try_recv().expect("detonation missing")
    else {
        panic!("unexpected report");
    };
    assert!(report.hits.is_empty());
    assert!((report.pos.x - 20.0 * TICK_SECS).abs() < 1e-5);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn a_retired_body_kept_for_the_death_camera_does_not_block_flight() {
    let (mut app, mut receiver) = app(10);
    let id = PlayerId(1);
    let pos = Position { x: 5.0, y: 0.0, z: 0.0 };
    let target = app.world_mut().spawn((PlayerMarker, id, pos, FaceYaw(0.0))).id();
    let player = Player::new("Player".into(), pos, PlayerMoveIntent::Idle, 0.0, 0, Health(0.0));
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::from_snapshot(target, &player, 0));
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .retire_body(id, player.generation);
    let entity = missile(&mut app, MissileId(3), true, 300.0, 10.0);
    app.update();
    assert!(app.world().get::<Position>(entity).expect("flight hit the dead body").x > 9.0);
    let ClientToServer::Send(ClientMessage::MissileMoves(_)) = receiver.try_recv().expect("movement report missing")
    else {
        panic!("flight detonated against the dead body");
    };
}
