use crate::missiles::{
    AirGraph, MissileFlight, MissileInfo, MissileMap, MissileVelocity, OwnedMissile, RemoteMissileMotion,
    missiles_movement_system,
};
use crate::{
    actors::ActorMap,
    carriers::CarrierEntities,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::{ClientToServer, ClientToServerChannel, SampleTiming},
    players::{PlayerInfo, PlayerMap},
    test_fixtures,
    vfx::{BlastRadii, ExplosionAssets, ExplosionVfxBudget},
};
use bevy::prelude::*;
use common::{config::NetworkConfig, constants::TICK_SECS, map::Carriers, physics::CollisionWorld, protocol::*};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

fn app(hz: u32) -> (App, UnboundedReceiver<ClientToServer>) {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
    app.init_asset::<AudioSource>();
    let (sender, receiver) = unbounded_channel();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(TICK_SECS));
    let layout = MapLayout::default();
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials);
    app.insert_resource(time)
        .insert_resource(meshes)
        .insert_resource(materials)
        .insert_resource(explosion_assets)
        .insert_resource(CarrierEntities::new(Vec::new()))
        .insert_resource(layout.clone())
        .insert_resource(AssetSet::load_default().expect("asset catalog invalid"))
        .insert_resource(ClientSettings::load_default().expect("client settings invalid"))
        .init_resource::<ExplosionVfxBudget>()
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
    if owned {
        app.world_mut().entity_mut(entity).insert(OwnedMissile {
            flight: MissileFlight::new(PlayerId(1), None, 0.0, lifetime),
            seq: 0,
        });
    } else {
        app.world_mut().entity_mut(entity).insert(RemoteMissileMotion::new(
            0,
            MissileMovementState::from_velocity(pos, velocity),
            SampleTiming {
                delay_ticks: 6.0,
                interval_ticks: 3.0,
            },
        ));
    }
    app.world_mut().resource_mut::<MissileMap>().insert(
        id,
        MissileInfo {
            entity,
            shooter: PlayerId(if owned { 1 } else { 2 }),
            born_tick: 0,
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

#[test]
fn a_missile_inside_geometry_detonates_where_it_is() {
    let (mut app, mut receiver) = app(10);
    let layout = MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -4.0,
            x2: 0.0,
            z2: 4.0,
            width: 1.0,
            y: 0.0,
            height: 4.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()));
    let entity = missile(&mut app, MissileId(1), true, 20.0, 10.0);
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    let ClientToServer::Send(ClientMessage::MissileDetonated(report)) =
        receiver.try_recv().expect("detonation missing")
    else {
        panic!("unexpected report");
    };
    assert_eq!(report.pos, Position { x: 0.0, y: 1.0, z: 0.0 });
}

#[test]
fn a_missile_arms_against_its_shooter_only_after_leaving_them() {
    let (mut app, mut receiver) = app(10);
    let id = PlayerId(1);
    let shooter = app
        .world_mut()
        .spawn((PlayerMarker, id, Position::default(), FaceYaw(0.0)))
        .id();
    let player = Player::new(
        "Player".into(),
        Position::default(),
        PlayerMoveIntent::Idle,
        0.0,
        0,
        Health(100.0),
    );
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::from_snapshot(shooter, &player, 0));
    let entity = missile(&mut app, MissileId(1), true, 20.0, 10.0);
    app.update();
    assert!(
        app.world().get_entity(entity).is_ok(),
        "the launch detonated inside its own shooter"
    );
    app.update();
    assert!(
        app.world()
            .get::<OwnedMissile>(entity)
            .expect("flight missing")
            .flight
            .armed
    );
    app.world_mut()
        .entity_mut(shooter)
        .insert(Position { x: 1.6, ..default() });
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    let detonation = std::iter::from_fn(|| receiver.try_recv().ok())
        .any(|message| matches!(message, ClientToServer::Send(ClientMessage::MissileDetonated(_))));
    assert!(detonation, "an armed missile passed through its shooter");
}

#[test]
fn a_missile_that_stops_making_progress_detonates_itself() {
    let (mut app, mut receiver) = app(10);
    let stall_secs = test_fixtures::gameplay_config().missiles.stall_secs;
    let entity = missile(&mut app, MissileId(1), true, 0.0, stall_secs * 10.0);
    for _ in 0..(stall_secs / TICK_SECS) as usize + 3 {
        app.update();
    }
    assert!(app.world().get_entity(entity).is_err());
    let detonation = std::iter::from_fn(|| receiver.try_recv().ok())
        .any(|message| matches!(message, ClientToServer::Send(ClientMessage::MissileDetonated(_))));
    assert!(detonation, "a wedged missile flew on forever");
}

#[test]
fn a_dead_target_clears_the_missiles_lock() {
    let (mut app, mut receiver) = app(10);
    let id = PlayerId(2);
    let pos = Position {
        x: 40.0,
        y: 0.0,
        z: 0.0,
    };
    let target = app.world_mut().spawn((PlayerMarker, id, pos, FaceYaw(0.0))).id();
    let player = Player::new("Player".into(), pos, PlayerMoveIntent::Idle, 0.0, 0, Health(100.0));
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::from_snapshot(target, &player, 0));
    let entity = missile(&mut app, MissileId(1), true, 20.0, 10.0);
    app.world_mut()
        .get_mut::<OwnedMissile>(entity)
        .expect("flight missing")
        .flight
        .target = Some(HomingTarget::Player(id));
    app.update();
    assert_eq!(
        app.world()
            .get::<OwnedMissile>(entity)
            .expect("flight missing")
            .flight
            .target,
        Some(HomingTarget::Player(id))
    );
    app.world_mut().resource_mut::<PlayerMap>().remove(&id);
    app.update();
    assert!(
        app.world()
            .get::<OwnedMissile>(entity)
            .expect("flight missing")
            .flight
            .target
            .is_none()
    );
    while receiver.try_recv().is_ok() {}
}
