use crate::config::fixtures;
use bevy::prelude::*;
use common::{map::Carriers, protocol::CarrierId};
use crossbeam_channel::unbounded;

use crate::{
    config::{PowerUpsConfig, ServerGameplayConfig},
    items::{ItemInfo, ItemMap, ItemPlacement, item_collection_system},
    players::{PlayerInfo, PlayerMap, PowerUpState},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    config::GameplayConfig,
    protocol::{
        BarrierKindId, Health, ItemId, ItemMarker, ItemType, PlayerId, PlayerMarker, Position, PowerUpKind,
        ServerMessage,
    },
};

fn test_app() -> App {
    let server = fixtures::server_config();
    let gameplay = server.gameplay_config();
    let power_ups = server.maps[&server.default_map].power_ups.clone();
    let placed_items = server
        .maps
        .get(&server.default_map)
        .expect("default map settings missing")
        .placed_items
        .clone();
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

fn spawn_player(app: &mut App, id: PlayerId, pos: Position) -> (Entity, crossbeam_channel::Receiver<ServerMessage>) {
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

fn random(spawned_at: f32) -> ItemPlacement {
    ItemPlacement::Random { spawned_at }
}

#[test]
fn permanent_single_shot_pickup_grants_fire_and_leaves_duplicates_for_other_players() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<PowerUpsConfig>()
        .duration_secs
        .single_shot = 0.0;
    let id = PlayerId(1);
    let (_, rx) = spawn_player(&mut app, id, Position::default());
    assert!(
        !app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&id)
            .expect("player missing")
            .has(PowerUpKind::SingleShot)
    );
    let first = spawn_item(
        &mut app,
        1,
        ItemType::SingleShotPowerUp,
        Position::default(),
        random(0.0),
    );
    app.update();
    assert!(app.world().resource::<ItemMap>().get(&first).is_none());
    assert!(std::iter::from_fn(|| rx.try_recv().ok()).any(|message| matches!(
        message,
        ServerMessage::PlayerStatus(status)
            if status.collected == Some(ItemType::SingleShotPowerUp)
                && status.power_up(PowerUpKind::SingleShot)
                && !status.power_up(PowerUpKind::MultiShot)
    )));
    let mut players = app.world_mut().resource_mut::<PlayerMap>();
    let info = players.get_mut(&id).expect("player missing");
    assert!(info.has(PowerUpKind::SingleShot));
    assert!(!info.has(PowerUpKind::MultiShot));
    let second = spawn_item(
        &mut app,
        2,
        ItemType::SingleShotPowerUp,
        Position::default(),
        random(0.0),
    );
    app.update();
    assert!(app.world().resource::<ItemMap>().get(&second).is_some());
}

#[test]
fn overlapping_gold_is_collected_and_scores() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, rx) = spawn_player(&mut app, id, Position::default());
    let item = spawn_item(&mut app, 1, ItemType::Gold, Position::default(), random(0.0));

    app.update();

    assert!(app.world().resource::<ItemMap>().get(&item).is_none(), "gold consumed");
    let expected = app.world().resource::<ServerGameplayConfig>().scoring.gold;
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("player present")
            .session
            .score,
        expected
    );
    let gold_cue = std::iter::from_fn(|| rx.try_recv().ok()).any(|msg| matches!(msg, ServerMessage::GoldCollected(_)));
    assert!(gold_cue, "pickup cue must be unicast");
}

#[test]
fn dead_player_collects_nothing() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("player present")
        .begin_respawn(1.0);
    let item = spawn_item(&mut app, 1, ItemType::Gold, Position::default(), random(0.0));

    app.update();

    assert!(
        app.world().resource::<ItemMap>().get(&item).is_some(),
        "a same-tick corpse must not vacuum up items"
    );
}

#[test]
fn permanent_power_up_stays_for_other_players_and_timed_pickup_refreshes() {
    let mut app = test_app();
    let mut config = app.world_mut().resource_mut::<PowerUpsConfig>();
    config.duration_secs.portal_gun = 0.0;
    config.duration_secs.speed = 30.0;
    let id = PlayerId(1);
    let (_, rx) = spawn_player(&mut app, id, Position::default());
    spawn_item(
        &mut app,
        1,
        ItemType::PortalGunPowerUp,
        Position::default(),
        random(0.0),
    );
    app.update();
    assert!(std::iter::from_fn(|| rx.try_recv().ok()).any(|message| matches!(
        message,
        ServerMessage::PlayerStatus(status)
            if status.collected == Some(ItemType::PortalGunPowerUp)
    )));
    let second = spawn_item(
        &mut app,
        2,
        ItemType::PortalGunPowerUp,
        Position::default(),
        random(0.0),
    );
    app.update();
    assert!(app.world().resource::<ItemMap>().get(&second).is_some());
    let mut players = app.world_mut().resource_mut::<PlayerMap>();
    let info = players.get_mut(&id).expect("player missing");
    assert!(info.has_permanent(PowerUpKind::PortalGun));
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.0);
    spawn_item(&mut app, 3, ItemType::SpeedPowerUp, Position::default(), random(0.0));
    app.update();
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("player missing")
            .life
            .power_ups[PowerUpKind::Speed.index()],
        PowerUpState::Timed(30.0)
    );
}

#[test]
fn eraser_wins_over_same_tick_pickup_and_does_not_repeat_status() {
    use crate::players::{erase_equipment_system, handle_move_outcome};
    use common::protocol::{CMoveOutcome, MoveOutcome, PlayerGeneration};
    use common::{
        physics::CollisionWorld,
        protocol::{Eraser, MapLayout, PowerUpKind},
    };
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
    spawn_item(
        &mut app,
        1,
        ItemType::PortalGunPowerUp,
        Position::default(),
        random(0.0),
    );
    let missile_pack = spawn_item(&mut app, 4, ItemType::MissilePack, Position::default(), random(0.0));
    spawn_item(
        &mut app,
        5,
        ItemType::Key(BarrierKindId(0)),
        Position::default(),
        random(0.0),
    );
    spawn_item(
        &mut app,
        2,
        ItemType::SingleShotPowerUp,
        Position::default(),
        random(0.0),
    );
    spawn_item(
        &mut app,
        3,
        ItemType::MultiShotPowerUp,
        Position::default(),
        random(0.0),
    );
    app.update();
    let info = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
    assert!(!info.has(PowerUpKind::PortalGun));
    assert!(!info.has(PowerUpKind::SingleShot));
    assert!(!info.has(PowerUpKind::MultiShot));
    assert_eq!(info.life.missiles, 0);
    assert_eq!(info.life.held_keys, [BarrierKindId(0)]);
    assert!(app.world().resource::<ItemMap>().get(&missile_pack).is_none());
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
    assert_eq!(status.held_keys, [BarrierKindId(0)]);
    app.update();
    assert!(rx.try_recv().is_err());
}

#[test]
fn already_held_key_is_left_in_the_world() {
    let mut app = test_app();
    let id = PlayerId(1);
    let kind = BarrierKindId(0);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    assert!(
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&id)
            .expect("player present")
            .add_key(kind)
    );
    let item = spawn_item(&mut app, 1, ItemType::Key(kind), Position::default(), random(0.0));

    app.update();

    assert!(
        app.world().resource::<ItemMap>().get(&item).is_some(),
        "the world key stays for a player who can use it"
    );
}

#[test]
fn full_missile_inventory_leaves_the_pack() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    let max = app.world().resource::<GameplayConfig>().missiles.max_missiles;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("player present")
        .life
        .missiles = max;
    let item = spawn_item(&mut app, 1, ItemType::MissilePack, Position::default(), random(0.0));

    app.update();
    assert!(
        app.world().resource::<ItemMap>().get(&item).is_some(),
        "a full player leaves the pack"
    );

    // With room, the same pack collects.
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("player present")
        .life
        .missiles = 0;
    app.update();
    assert!(app.world().resource::<ItemMap>().get(&item).is_none(), "pack collected");
}

#[test]
fn hidden_placed_item_is_not_collectable() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    let item = spawn_item(
        &mut app,
        1,
        ItemType::Gold,
        Position::default(),
        ItemPlacement::Placed { respawn_countdown: 5.0 },
    );

    app.update();

    assert!(
        app.world().resource::<ItemMap>().get(&item).is_some(),
        "an item mid-respawn-countdown is uncollectable"
    );
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("player present")
            .session
            .score,
        0
    );
}

#[test]
fn item_on_another_floor_is_not_collected() {
    let mut app = test_app();
    let id = PlayerId(1);
    let (_, _rx) = spawn_player(&mut app, id, Position::default());
    let above = Position { x: 0.0, y: 0.2, z: 0.0 };
    let item = spawn_item(&mut app, 1, ItemType::Gold, above, random(0.0));

    app.update();

    assert!(
        app.world().resource::<ItemMap>().get(&item).is_some(),
        "vertical epsilon keeps cross-floor pickups out"
    );
}
