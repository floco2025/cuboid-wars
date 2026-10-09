use std::time::Duration;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, unbounded};
use serde_json::json;

use super::*;
use crate::{
    config::{PlacedItemsConfig, PowerUpMode, PowerUpsConfig, RandomItemsConfig, ServerGameplayConfig, fixtures},
    items::{
        ItemInfo, ItemMap, ItemPlacement, ItemSpawner, RandomItems, placed_item_respawn_system,
        random_item_spawn_system,
    },
    map::MapConfig,
    players::{PlayerInfo, PlayerMap, PowerUpState, erase_equipment_system, handle_move_outcome},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog},
    test_geometry::{floored_level, geometry},
};
use common::{
    config::GameplayConfig,
    constants::{CHARACTER_CONTACT_OFFSET, PRESSURE_PLATE_HEIGHT},
    map::Carriers,
    physics::CollisionWorld,
    protocol::{
        CMoveOutcome, CarrierId, Eraser, FieldId, Health, ItemId, ItemMarker, ItemType, MapLayout, MoveOutcome,
        PlayerGeneration, PlayerId, PlayerMarker, Portal, PortalEnd, PortalMode, Position, PowerUpKind, ServerMessage,
    },
};

#[test]
fn pickups_without_effect_stay_in_the_world() {
    let server_config = fixtures::server_config();
    let config = server_config.gameplay_config();
    let max_health = server_config.combat.health.player.max;
    let effect = |item, player: &PlayerInfo, holds_portals, health: Option<f32>| {
        pickup_has_effect(
            item,
            player,
            holds_portals,
            health.map(Health).as_ref(),
            &config,
            &server_config,
        )
    };
    let (tx, _rx) = unbounded();
    let mut player = PlayerInfo::new(Entity::PLACEHOLDER, tx);

    assert!(!effect(ItemType::HealthPotion, &player, false, Some(max_health)));
    assert!(effect(ItemType::HealthPotion, &player, false, Some(max_health / 2.0)));

    assert!(effect(ItemType::MissilePack, &player, false, None));
    player.add_missiles(config.missiles.max_missiles, config.missiles.max_missiles);
    assert!(!effect(ItemType::MissilePack, &player, false, None));

    assert!(player.add_key(FieldId(0)));
    assert!(!effect(ItemType::Key(FieldId(0)), &player, false, None));
    assert!(effect(ItemType::Key(FieldId(1)), &player, false, None));

    let (tx, _rx) = unbounded();
    let empty_handed = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    for holds_portals in [false, true] {
        assert_eq!(
            effect(ItemType::EquipmentEraser, &empty_handed, holds_portals, None),
            holds_portals,
            "an eraser with nothing to take but open portals: {holds_portals}"
        );
    }
}

fn test_app() -> App {
    let server = fixtures::server_config();
    let gameplay = server.gameplay_config();
    let power_ups = server.power_ups.clone();
    let placed_items = server.placed_items.clone().unwrap_or_default();
    let quest_catalog = QuestCatalog::from_config(&server);
    let quest_board = QuestBoard::from_catalog(&quest_catalog, None);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(server)
        .insert_resource(quest_catalog)
        .insert_resource(quest_board)
        .insert_resource(gameplay)
        .insert_resource(placed_items)
        .insert_resource(power_ups)
        .insert_resource(PlayerMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(Carriers::default())
        .add_systems(Update, item_collection_system);
    app
}

fn spawn_player(app: &mut App, id: PlayerId, pos: Position) -> (Entity, Receiver<ServerMessage>) {
    let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(50.0))).id();
    let (sender, receiver) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    (entity, receiver)
}

fn spawn_item(app: &mut App, id: u32, item_type: ItemType, pos: Position, placement: ItemPlacement) -> ItemId {
    let entity = app.world_mut().spawn((ItemMarker, pos)).id();
    app.world_mut().resource_mut::<ItemMap>().insert(
        ItemId(id),
        ItemInfo {
            entity,
            item_type,
            placement,
            carrier: CarrierId::WORLD,
        },
    );
    ItemId(id)
}

fn spawn_random(app: &mut App, id: u32, item_type: ItemType) -> ItemId {
    spawn_item(app, id, item_type, Position::default(), random(0.0))
}

fn random(spawned_at: f32) -> ItemPlacement {
    ItemPlacement::Random { spawned_at }
}

fn info(app: &App, id: PlayerId) -> &PlayerInfo {
    app.world().resource::<PlayerMap>().get(&id).expect("player missing")
}

fn info_mut(app: &mut App, id: PlayerId) -> &mut PlayerInfo {
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .into_inner()
        .get_mut(&id)
        .expect("player missing")
}

fn present(app: &App, item: ItemId) -> bool {
    app.world().resource::<ItemMap>().get(&item).is_some()
}

fn erasure_cues(rx: &Receiver<ServerMessage>) -> usize {
    rx.try_iter()
        .filter(|message| matches!(message, ServerMessage::EquipmentErased(_)))
        .count()
}

fn simultaneous_pickup_app(player_count: u32, item_type: ItemType, placed: bool) -> App {
    let mut app = test_app();
    {
        let mut config = app.world_mut().resource_mut::<ServerGameplayConfig>();
        config.combat.health.player.max = 100.0;
        config.combat.health.player.potion_heal = 0.25;
        config.weapons.missiles.missiles_per_pack = 3;
    }
    app.world_mut().resource_mut::<GameplayConfig>().missiles.max_missiles = 6;
    app.world_mut().resource_mut::<PowerUpsConfig>().portal_gun = PowerUpMode::Pickup { duration_secs: None };
    app.insert_resource(PlacedItemsConfig::default());
    for id in 1..=player_count {
        let id = PlayerId(id);
        let (entity, _) = spawn_player(&mut app, id, Position::default());
        app.world_mut().entity_mut(entity).insert(Health(99.0));
        info_mut(&mut app, id).life.missiles = 5;
    }
    for (id, x) in [(1, -0.6), (2, 0.6)] {
        spawn_item(
            &mut app,
            id,
            item_type,
            Position { x, ..default() },
            if placed {
                ItemPlacement::Placed {
                    respawn_countdown: Some(0.0),
                }
            } else {
                random(0.0)
            },
        );
    }
    app
}

fn assert_pickup_received(app: &App, id: PlayerId, item_type: ItemType) {
    let player = info(app, id);
    match item_type {
        ItemType::HealthPotion => assert_eq!(
            app.world().get::<Health>(player.entity().expect("player body missing")),
            Some(&Health(100.0))
        ),
        ItemType::MissilePack => assert_eq!(player.life.missiles, 6),
        ItemType::Key(kind) => assert_eq!(player.life.held_keys, vec![kind]),
        ItemType::PortalGunPowerUp => assert!(player.has_permanent(PowerUpKind::PortalGun)),
        _ => panic!("unexpected test pickup"),
    }
}

#[test]
fn simultaneous_pickups_leave_redundant_items_available() {
    for item_type in [
        ItemType::HealthPotion,
        ItemType::MissilePack,
        ItemType::Key(FieldId(0)),
        ItemType::PortalGunPowerUp,
    ] {
        for placed in [false, true] {
            let mut app = simultaneous_pickup_app(1, item_type, placed);
            app.update();
            assert_eq!(
                app.world()
                    .resource::<ItemMap>()
                    .values()
                    .filter(|item| !item.is_hidden())
                    .count(),
                1,
                "{item_type:?}, placed={placed}"
            );
            assert_pickup_received(&app, PlayerId(1), item_type);
        }
    }
}

#[test]
fn simultaneous_pickups_can_help_two_overlapping_players() {
    for item_type in [
        ItemType::HealthPotion,
        ItemType::MissilePack,
        ItemType::Key(FieldId(0)),
        ItemType::PortalGunPowerUp,
    ] {
        let mut app = simultaneous_pickup_app(2, item_type, false);
        app.update();
        assert_eq!(app.world().resource::<ItemMap>().values().count(), 0, "{item_type:?}");
        for id in [PlayerId(1), PlayerId(2)] {
            assert_pickup_received(&app, id, item_type);
        }
    }
}

#[test]
fn successive_potions_and_packs_are_consumed_while_useful() {
    for item_type in [ItemType::HealthPotion, ItemType::MissilePack] {
        let mut app = simultaneous_pickup_app(1, item_type, false);
        let player = info_mut(&mut app, PlayerId(1));
        player.life.missiles = 0;
        let entity = player.entity().expect("player body missing");
        app.world_mut().entity_mut(entity).insert(Health(50.0));
        app.update();
        assert_eq!(app.world().resource::<ItemMap>().values().count(), 0, "{item_type:?}");
        assert_pickup_received(&app, PlayerId(1), item_type);
    }
}

#[test]
fn placed_gold_obeys_never_immediate_and_delayed_respawn_settings() {
    for (settings, expected_collections) in [
        (json!(null), [1, 1, 1]),
        (json!({"respawn_secs": {}}), [1, 1, 1]),
        (json!({"respawn_secs": {"gold": null}}), [1, 1, 1]),
        (json!({"respawn_secs": {"gold": 0}}), [1, 2, 3]),
        (json!({"respawn_secs": {"gold": 2}}), [1, 1, 2]),
    ] {
        let mut app = test_app();
        let config: Option<PlacedItemsConfig> =
            serde_json::from_value(settings.clone()).expect("placed item settings invalid");
        app.insert_resource(config.unwrap_or_default());
        let id = PlayerId(1);
        spawn_player(&mut app, id, Position::default());
        let item_id = spawn_item(
            &mut app,
            1,
            ItemType::Gold,
            Position::default(),
            ItemPlacement::Placed {
                respawn_countdown: Some(0.0),
            },
        );
        let gold_score = app.world().resource::<ServerGameplayConfig>().scoring.gold;
        let mut respawn = Schedule::default();
        respawn.add_systems(placed_item_respawn_system);

        for collections in expected_collections {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(1));
            respawn.run(app.world_mut());
            app.update();
            assert_eq!(
                info(&app, id).session.score,
                collections * gold_score,
                "wrong collection count for {settings}",
            );
            let item = app
                .world()
                .resource::<ItemMap>()
                .get(&item_id)
                .expect("placed item removed");
            assert!(
                app.world().get::<ItemMarker>(item.entity).is_some(),
                "placed cell reservation lost"
            );
        }
    }
}

#[test]
fn collected_random_item_is_replaced_in_the_same_tick() {
    let mut app = test_app();
    let geometry = geometry(2, 1);
    let config = RandomItemsConfig {
        weights: [("gold".to_owned(), 1.0)].into(),
        max_number: 2,
        despawn_secs: 10.0,
    };
    app.insert_resource(MapConfig::for_grid(vec![floored_level(2, 1)], geometry))
        .insert_resource(geometry)
        .insert_resource(ItemSpawner::default())
        .insert_resource(RandomItems::from_config(Some(&config)))
        .add_systems(Update, random_item_spawn_system.after(item_collection_system));
    app.update();
    let (&id, item) = app
        .world()
        .resource::<ItemMap>()
        .iter()
        .next()
        .expect("random item missing");
    let position = *app.world().get::<Position>(item.entity).expect("item position missing");
    let (_, receiver) = spawn_player(&mut app, PlayerId(1), position);

    app.update();

    let items = app.world().resource::<ItemMap>();
    assert!(items.get(&id).is_none());
    assert_eq!(items.iter().count(), 2);
    assert!(
        receiver
            .try_iter()
            .any(|message| matches!(message, ServerMessage::GoldCollected(_)))
    );
}

#[test]
fn dead_player_collects_nothing() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    info_mut(&mut app, id).begin_respawn(1.0);
    let item = spawn_random(&mut app, 1, ItemType::Gold);

    app.update();

    assert!(present(&app, item), "a same-tick corpse must not vacuum up items");
}

#[test]
fn permanent_power_up_stays_for_other_players_and_timed_pickup_refreshes() {
    let mut app = test_app();
    let mut config = app.world_mut().resource_mut::<PowerUpsConfig>();
    config.portal_gun = PowerUpMode::Pickup { duration_secs: None };
    config.speed = PowerUpMode::Pickup {
        duration_secs: Some(30.0),
    };
    let id = PlayerId(1);
    let (_, rx) = spawn_player(&mut app, id, Position::default());
    spawn_random(&mut app, 1, ItemType::PortalGunPowerUp);
    app.update();
    assert!(rx.try_iter().any(|message| matches!(
        message,
        ServerMessage::PlayerStatus(status)
            if status.collected == Some(ItemType::PortalGunPowerUp)
    )));
    let second = spawn_random(&mut app, 2, ItemType::PortalGunPowerUp);
    app.update();
    assert!(present(&app, second));
    let player = info_mut(&mut app, id);
    assert!(player.has_permanent(PowerUpKind::PortalGun));
    player.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.0);
    spawn_random(&mut app, 3, ItemType::SpeedPowerUp);
    app.update();
    assert_eq!(
        info(&app, id).life.power_ups[PowerUpKind::Speed.index()],
        PowerUpState::Timed(30.0)
    );
}

#[test]
fn eraser_wins_over_same_tick_pickup_and_does_not_repeat_status() {
    let mut app = test_app();
    let layout = MapLayout {
        erasers: vec![Eraser {
            x1: -2.0,
            z1: 0.0,
            x2: 2.0,
            z2: 0.0,
            width: 0.1,
            y: 0.0,
            height: 4.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout))
        .add_systems(Update, erase_equipment_system.after(item_collection_system));
    let id = PlayerId(1);
    let (entity, rx) = spawn_player(&mut app, id, Position::default());
    handle_move_outcome(
        id,
        CMoveOutcome {
            generation: PlayerGeneration(0),
            event: MoveOutcome::EraseEquipment,
        },
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
    spawn_random(&mut app, 1, ItemType::PortalGunPowerUp);
    spawn_random(&mut app, 2, ItemType::SingleShotPowerUp);
    spawn_random(&mut app, 3, ItemType::MultiShotPowerUp);
    let missile_pack = spawn_random(&mut app, 4, ItemType::MissilePack);
    spawn_random(&mut app, 5, ItemType::Key(FieldId(0)));
    app.update();
    let player = info(&app, id);
    assert!(!player.has(PowerUpKind::PortalGun));
    assert!(!player.has(PowerUpKind::SingleShot));
    assert!(!player.has(PowerUpKind::MultiShot));
    assert_eq!(player.life.missiles, 0);
    assert_eq!(player.life.held_keys, [FieldId(0)]);
    assert!(!present(&app, missile_pack));
    assert_eq!(app.world().get::<Health>(entity), Some(&Health(50.0)));
    let mut last = None;
    let mut collected = false;
    while let Ok(message) = rx.try_recv() {
        if let ServerMessage::PlayerStatus(status) = message {
            collected |= status.collected.is_some();
            last = Some(status);
        }
    }
    let status = last.expect("erasure status missing");
    assert!(collected);
    assert!(status.collected.is_none());
    assert!(!status.power_up(PowerUpKind::PortalGun));
    assert!(!status.power_up(PowerUpKind::SingleShot));
    assert!(!status.power_up(PowerUpKind::MultiShot));
    assert_eq!(status.missiles, 0);
    assert_eq!(status.held_keys, [FieldId(0)]);
    app.update();
    assert!(rx.try_recv().is_err());
}

#[test]
fn player_standing_on_a_pressure_plate_collects_the_item_on_its_cell() {
    let mut app = test_app();
    let on_plate = Position {
        y: PRESSURE_PLATE_HEIGHT + CHARACTER_CONTACT_OFFSET,
        ..default()
    };
    let (_, _rx) = spawn_player(&mut app, PlayerId(1), on_plate);
    let item = spawn_random(&mut app, 1, ItemType::Gold);

    app.update();

    assert!(!present(&app, item));
}

#[test]
fn suspended_eraser_pickup_clears_collected_equipment_and_preserves_permanent_map_abilities() {
    let mut app = test_app();
    app.add_systems(Update, erase_equipment_system.after(item_collection_system));
    app.world_mut()
        .resource_mut::<PlacedItemsConfig>()
        .respawn_secs
        .equipment_eraser = Some(0.0);
    let mut always = [false; PowerUpKind::COUNT];
    always[PowerUpKind::PortalGun.index()] = true;
    app.insert_resource(PlayerMap::new(Default::default(), always));
    let id = PlayerId(1);
    let (entity, rx) = spawn_player(&mut app, id, Position::default());
    {
        let player = info_mut(&mut app, id);
        player.add_key(FieldId(0));
        player.life.missiles = 2;
        player.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Permanent;
        player.life.power_ups[PowerUpKind::LowGravity.index()] = PowerUpState::Timed(30.0);
    }
    let eraser = spawn_item(
        &mut app,
        1,
        ItemType::EquipmentEraser,
        Position { y: 4.4, ..default() },
        ItemPlacement::Placed {
            respawn_countdown: Some(0.0),
        },
    );
    app.update();
    assert!(info(&app, id).has_speed());
    app.world_mut()
        .entity_mut(entity)
        .insert(Position { y: 4.4, ..default() });
    app.update();
    let player = info(&app, id);
    assert!(!player.has_speed());
    assert!(!player.has_low_gravity());
    assert_eq!(player.life.missiles, 0);
    assert!(player.has(PowerUpKind::PortalGun));
    assert_eq!(player.life.held_keys, [FieldId(0)]);
    assert_eq!(app.world().get::<Health>(entity), Some(&Health(50.0)));
    assert!(
        !app.world()
            .resource::<ItemMap>()
            .get(&eraser)
            .expect("placed eraser")
            .is_hidden()
    );
    assert_eq!(erasure_cues(&rx), 1);
    app.update();
    assert_eq!(erasure_cues(&rx), 0);
    info_mut(&mut app, id).life.missiles = 1;
    app.update();
    assert_eq!(info(&app, id).life.missiles, 0);
    assert_eq!(erasure_cues(&rx), 1);
}

#[test]
fn eraser_pickup_waits_for_equipment_and_wins_over_a_same_tick_boost() {
    let mut app = test_app();
    app.add_systems(Update, erase_equipment_system.after(item_collection_system));
    let id = PlayerId(1);
    let (_, rx) = spawn_player(&mut app, id, Position::default());
    let eraser = spawn_random(&mut app, 1, ItemType::EquipmentEraser);
    app.update();
    assert!(present(&app, eraser));
    spawn_random(&mut app, 2, ItemType::SpeedPowerUp);
    app.update();
    assert!(!info(&app, id).has_speed());
    assert!(!present(&app, eraser));
    assert_eq!(erasure_cues(&rx), 1);
}

#[test]
fn an_eraser_pickup_is_taken_by_an_empty_handed_player_with_open_portals() {
    let mut app = test_app();
    let id = PlayerId(1);
    spawn_player(&mut app, id, Position::default());
    let mut assignments = PortalAssignments::new(PortalMode::Both);
    assignments.assign(id);
    let mut portals = PortalMap::default();
    portals.set(Portal {
        pair: assignments.get(&id).pair().expect("portal pair missing"),
        end: PortalEnd::A,
        pos: Position::default(),
        nx: 0.0,
        ny: 0.0,
        nz: 1.0,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    });
    app.insert_resource(assignments).insert_resource(portals);
    let eraser = spawn_random(&mut app, 1, ItemType::EquipmentEraser);
    app.update();
    assert!(!present(&app, eraser));
    assert!(info(&app, id).life.outcomes.erase_equipment);
}
