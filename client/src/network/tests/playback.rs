use super::super::playback::*;
use crate::{
    actors::{ActorGhostMap, ActorMap},
    barriers::LockedSwitches,
    carriers::{CarrierEntities, CarrierStoreys},
    characters::MaxHealth,
    fields::{SharedCheckpoint, build_field_assets},
    input::PendingWeaponSelection,
    items::{ItemMap, setup_item_assets},
    map::MapDimensions,
    missiles::{MissileAssets, MissileMap},
    network::{LastPlayerMovesTick, RoundTripTime, TickSync},
    players::MyPlayerId,
    portals::{PortalAssets, PortalMap},
    test_fixtures,
    ui::{HudBanner, MessageFeed, QuestLog},
    vfx::{BlastRadii, ExplosionAssets, ExplosionVfxBudget, PortalFizzleAssets, WeatherIntensity},
};
use bevy::{ecs::system::RunSystemOnce, gltf::Gltf};
use common::{
    celestial::CelestialClockAnchor,
    config::NetworkConfig,
    map::Carriers,
    physics::{CollisionWorld, PortalSet},
};

fn fixture() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<Image>()
        .init_asset::<Gltf>();
    app.insert_resource(test_fixtures::client_settings())
        .insert_resource(test_fixtures::asset_set());
    let gameplay = test_fixtures::gameplay_config();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let field_assets = build_field_assets(&mut meshes, &mut materials, &[], 1.0, 1.0);
    let projectile_assets = ProjectileAssets::new(&mut meshes, &mut materials, 0.08);
    app.insert_resource(field_assets)
        .insert_resource(projectile_assets)
        .insert_resource(meshes)
        .insert_resource(materials);
    let layout = MapLayout::default();
    let settings = test_fixtures::map_settings();
    let clock = CelestialClockAnchor::initial(&settings.celestial, 0);
    let carrier = app.world_mut().spawn_empty().id();
    app.insert_resource(CarrierEntities::new(vec![carrier]))
        .insert_resource(CarrierStoreys::from_layout(&layout))
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(MapDimensions {
            width: 30.0,
            depth: 30.0,
            height: 10.0,
        })
        .insert_resource(MaxHealth {
            player: 100.0,
            actors: Default::default(),
        })
        .insert_resource(layout)
        .insert_resource(settings)
        .insert_resource(gameplay)
        .insert_resource(clock)
        .insert_resource(MyPlayerId(PlayerId(1)))
        .insert_resource(PortalAccess::None)
        .init_resource::<NetworkConfig>()
        .init_resource::<Carriers>()
        .init_resource::<PortalSet>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<ActorGhostMap>()
        .init_resource::<ItemMap>()
        .init_resource::<MissileMap>()
        .init_resource::<PortalMap>()
        .init_resource::<PortalAssets>()
        .init_resource::<PortalFizzleAssets>()
        .init_resource::<MissileAssets>()
        .init_resource::<ExplosionAssets>()
        .init_resource::<ExplosionVfxBudget>()
        .init_resource::<BlastRadii>()
        .init_resource::<RoundTripTime>()
        .init_resource::<LastSnapshotTick>()
        .init_resource::<LastPlayerMovesTick>()
        .init_resource::<ServerTick>()
        .init_resource::<TickSync>()
        .init_resource::<LocalPlayerInfo>()
        .init_resource::<PendingWeaponSelection>()
        .init_resource::<QuestLog>()
        .init_resource::<HudBanner>()
        .init_resource::<MessageFeed>()
        .init_resource::<FireworkShow>()
        .init_resource::<SwitchState>()
        .init_resource::<LockedSwitches>()
        .init_resource::<SharedCheckpoint>()
        .init_resource::<WeatherIntensity>();
    app.world_mut().run_system_once(setup_item_assets).expect("item assets");
    install_playback(&mut app);
    app
}

fn snapshot() -> SSnapshot {
    SSnapshot {
        tick: 40,
        players: vec![(
            PlayerId(1),
            Player::new(
                "Viewer".into(),
                Position::default(),
                PlayerMoveIntent::Idle,
                0.0,
                0,
                Health(100.0),
            ),
        )],
        actors: vec![],
        spawning_actors: vec![],
        actors_peaceful: false,
        items: vec![],
        missiles: vec![],
        switch_state: SwitchState::default(),
        quests: vec![],
        shared_checkpoint: 0,
        locked_switches: vec![],
        cloud_cover: 0.0,
        raining: false,
        celestial_clock: CelestialClockAnchor::initial(&test_fixtures::map_settings().celestial, 0),
        portals: vec![],
    }
}

fn apply(world: &mut World, snapshot: SSnapshot, reset: bool, shots: Vec<(u64, Position)>) {
    apply_playback_frame(
        world,
        PlaybackFrame {
            snapshot,
            projectiles: shots,
            cues: vec![],
            reset,
        },
    );
}

#[test]
fn viewer_places_exact_owner_samples_and_restarts_after_death_without_old_guards() {
    let mut app = fixture();
    let world = app.world_mut();
    let mut sample = snapshot();
    sample.portals.push(Portal {
        pair: PortalPairId(0),
        end: PortalEnd::A,
        pos: Position {
            x: 10.0,
            y: 2.0,
            z: 0.0,
        },
        nx: 0.0,
        ny: 0.0,
        nz: 1.0,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    });
    sample.items.push((
        ItemId(1),
        Item {
            item_type: ItemType::Gold,
            pos: Position::default(),
            carrier: CarrierId::WORLD,
        },
    ));
    apply(world, sample.clone(), false, vec![]);
    assert_eq!(world.resource::<PortalMap>().wire_portals().len(), 1);
    assert!(world.resource::<ItemMap>().contains_key(&ItemId(1)));
    let entity = world
        .resource::<PlayerMap>()
        .get(&PlayerId(1))
        .expect("spawned player")
        .entity;
    world.resource_mut::<LocalPlayerInfo>().stored_yaw = 0.7;
    sample.players[0].1.movement.pos = Position {
        x: 12.0,
        y: 8.0,
        z: 4.0,
    };
    sample.players[0].1.movement.vertical_velocity = -5.0;
    sample.players[0].1.checkpoint = 3;
    // The tick is deliberately unchanged: an observation can also follow aim/inspect.
    apply(
        world,
        sample.clone(),
        false,
        vec![(0, sample.players[0].1.movement.pos)],
    );
    assert_eq!(
        *world.get::<Position>(entity).expect("position"),
        sample.players[0].1.movement.pos
    );
    assert_eq!(
        world.get::<PreviousTickPosition>(entity).expect("render anchor").0,
        sample.players[0].1.movement.pos
    );
    assert_eq!(world.resource::<LocalPlayerInfo>().stored_yaw, 0.7);
    assert_eq!(world.query::<&PlaybackProjectileMarker>().iter(world).count(), 1);
    sample.tick += 1;
    sample.players.clear();
    apply(world, sample, false, vec![]);
    assert!(world.resource::<LocalPlayerInfo>().is_dead);
    assert_eq!(
        *world.get::<Visibility>(entity).expect("hidden corpse"),
        Visibility::Hidden
    );
    let mut restarted = snapshot();
    restarted.tick = 1;
    apply(world, restarted, true, vec![]);
    assert!(world.get_entity(entity).is_err());
    let local = world.resource::<PlayerMap>().get(&PlayerId(1)).expect("new body");
    assert_eq!(local.checkpoint, 0);
    assert_eq!(local.generation, PlayerGeneration(0));
    assert_eq!(
        *world.get::<Position>(local.entity).expect("spawn position"),
        Position::default()
    );
    assert!(!world.resource::<LocalPlayerInfo>().is_dead);
    assert_eq!(world.resource::<LocalPlayerInfo>().stored_yaw, 0.7);
    assert_eq!(world.query::<&PlaybackProjectileMarker>().iter(world).count(), 0);
    assert!(world.resource::<PortalMap>().wire_portals().is_empty());
    assert!(!world.resource::<ItemMap>().contains_key(&ItemId(1)));
}

#[test]
fn playback_installation_stops_fixed_simulation() {
    #[derive(Resource, Default)]
    struct Counts(u32);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).init_resource::<Counts>();
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs(1),
    ));
    app.add_systems(FixedUpdate, |mut counts: ResMut<Counts>| counts.0 += 1);
    install_playback(&mut app);
    for _ in 0..5 {
        app.update();
    }
    let counts = app.world().resource::<Counts>();
    assert_eq!(counts.0, 0);
}
