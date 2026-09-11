use std::{f32::consts::FRAC_PI_2, time::Duration};

use bevy::{ecs::world::CommandQueue, prelude::*};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, PortalSet},
    protocol::*,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{
    LastBounceSound, ProjectileAssets, projectiles_movement_system, spawn_ember_projectile, spawn_projectiles,
};
use crate::{
    actors::{ActorInfo, ActorMap},
    barriers::build_barrier_assets,
    bridges::build_bridge_assets,
    config::{AssetSet, ClientSettings},
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    projectiles::{MuzzleCheck, ProjectileMarker},
    test_fixtures,
    vfx::ParticleClouds,
};

fn app() -> (App, UnboundedReceiver<ClientToServer>) {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
    app.init_asset::<AudioSource>();
    let gameplay = test_fixtures::gameplay_config();
    let settings = ClientSettings::load_default().expect("client settings invalid");
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let assets = ProjectileAssets::new(&mut meshes, &mut materials, gameplay.projectiles.radius);
    let barriers = build_barrier_assets(
        &mut meshes,
        &mut materials,
        &[],
        &MapLayout::default(),
        settings.vfx.barriers,
        1.0,
    );
    let bridges = build_bridge_assets(&mut materials, &[], &MapLayout::default(), settings.vfx.light_bridges);
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(1.0 / 30.0));
    let (sender, receiver) = unbounded_channel();
    app.insert_resource(time)
        .insert_resource(settings)
        .insert_resource(AssetSet::load_default().expect("asset catalog invalid"))
        .insert_resource(meshes)
        .insert_resource(materials)
        .insert_resource(assets)
        .insert_resource(barriers)
        .insert_resource(bridges)
        .insert_resource(gameplay)
        .insert_resource(test_fixtures::map_settings())
        .insert_resource(CollisionWorld::from_map_layout(&MapLayout::default()))
        .insert_resource(ClientToServerChannel::new(sender))
        .insert_resource(MyPlayerId(PlayerId(1)))
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<PlateState>()
        .init_resource::<PortalSet>()
        .init_resource::<LastBounceSound>()
        .init_resource::<ParticleClouds>()
        .init_resource::<LocalPlayerInfo>()
        .add_systems(Update, projectiles_movement_system);
    let entity = app
        .world_mut()
        .spawn((ActorMarker, ActorId(7), Position::default(), FaceYaw(0.0)))
        .id();
    app.world_mut().resource_mut::<ActorMap>().insert(
        ActorId(7),
        ActorInfo {
            entity,
            kind: "bruiser".into(),
            beam: Default::default(),
        },
    );
    (app, receiver)
}

fn shot() -> CProjectileShot {
    CProjectileShot {
        origin: Position {
            x: -2.0,
            y: 0.5,
            z: 0.0,
        },
        face_yaw: FRAC_PI_2,
        face_pitch: 0.0,
        pattern: 0,
    }
}

fn fire(app: &mut App, shooter: PlayerId, ember: bool) {
    let mut queue = CommandQueue::default();
    let world = app.world();
    let mut commands = Commands::new(&mut queue, world);
    if ember {
        spawn_ember_projectile(
            &mut commands,
            world.resource::<ProjectileAssets>(),
            world.resource::<GameplayConfig>(),
            shot().origin.into(),
            Vec3::X * 120.0,
            Some(shooter),
        );
    } else {
        assert_eq!(
            spawn_projectiles(
                &mut commands,
                world.resource::<ProjectileAssets>(),
                &shot(),
                world.resource::<GameplayConfig>(),
                120.0,
                world.resource::<CollisionWorld>(),
                &[],
                shooter,
                MuzzleCheck::Enforced,
            ),
            1
        );
    }
    queue.apply(app.world_mut());
}

#[test]
fn only_the_shooters_real_bullet_reports_a_hit_once_even_while_dead() {
    for (shooter, ember, reports) in [(PlayerId(1), false, 1), (PlayerId(2), false, 0), (PlayerId(1), true, 0)] {
        let (mut app, mut receiver) = app();
        app.world_mut().resource_mut::<LocalPlayerInfo>().is_dead = true;
        fire(&mut app, shooter, ember);
        app.update();
        app.update();
        let messages: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        assert_eq!(messages.len(), reports);
        for message in messages {
            let ClientToServer::Send(ClientMessage::ProjectileHit(hit)) = message else {
                panic!("unexpected report")
            };
            assert_eq!(hit.target, HitTarget::Actor(ActorId(7)));
            assert_eq!(ClientMessage::ProjectileHit(hit).lane(), Lane::Reliable);
        }
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<ProjectileMarker>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}

#[test]
fn cosmetic_bullets_without_a_shooter_record_finish_their_normal_lifetime() {
    let (mut app, mut receiver) = app();
    let actor = app
        .world()
        .resource::<ActorMap>()
        .get(&ActorId(7))
        .expect("actor missing")
        .entity;
    app.world_mut().despawn(actor);
    app.world_mut().resource_mut::<ActorMap>().remove(&ActorId(7));
    fire(&mut app, PlayerId(2), false);
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProjectileMarker>>()
            .iter(app.world())
            .count(),
        1
    );
    let lifetime = app.world().resource::<GameplayConfig>().projectiles.lifetime_secs;
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(lifetime));
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProjectileMarker>>()
            .iter(app.world())
            .count(),
        0
    );
    assert!(receiver.try_recv().is_err());
}
