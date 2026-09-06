use bevy::prelude::*;

use common::{
    map::MapGeometry,
    protocol::{BarrierKindId, CarrierId, ItemMarker, ItemType, MapSettings, MapWeaponSettings, PortalMode},
};

use crate::map::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlacedItem};
use crate::{
    config::RandomItemsConfig,
    items::{ItemMap, ItemSpawner, RandomItems},
    test_geometry::geometry,
};

use super::{
    spawn_cells::{ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, target_active_random_items},
    spawning::placed_item_spawn_system,
};

fn map_config(levels: Vec<LevelGrid>, geometry: MapGeometry) -> MapConfig {
    MapConfig::for_grid(levels, geometry)
}

fn level_grid(cells: CellGrid) -> LevelGrid {
    LevelGrid {
        cells,
        edges: EdgeGrid::new(1, 1),
        barrier_edges: EdgeGrid::new(1, 1),
    }
}

fn map_settings(projectiles: bool, missiles: bool) -> MapSettings {
    let mut settings = crate::config::ServerGameplayConfig::load_default()
        .expect("default server gameplay config should load")
        .maps
        .get("hotel")
        .expect("hotel map settings missing")
        .settings
        .clone();
    settings.skybox = "test".to_owned();
    settings.weapons = MapWeaponSettings {
        projectiles,
        missiles,
        portals: PortalMode::None,
    };
    settings
}

#[test]
fn item_spawn_cells_include_all_floor_levels_and_skip_ramps() {
    let mut lower = CellGrid::new(1, 1);
    lower.rows[0][0].has_floor = true;
    let mut upper = CellGrid::new(1, 1);
    upper.rows[0][0].has_floor = true;
    upper.rows[0][0].has_ramp = true;
    let config = map_config(vec![level_grid(lower), level_grid(upper)], geometry(1, 1));

    let cells = eligible_item_spawn_cells(config.root_grid());

    assert_eq!(
        cells,
        vec![ItemSpawnCell {
            level: 0,
            col: 0,
            row: 0
        }]
    );
}

#[test]
fn random_item_target_is_capped_by_eligible_cells() {
    // Empty / undersized maps degrade gracefully; once there's enough room
    // the count is just the configured `max_number`.
    let max_number = 50;
    assert_eq!(target_active_random_items(0, max_number), 0);
    assert_eq!(target_active_random_items(1, max_number), 1);
    assert_eq!(target_active_random_items(max_number - 1, max_number), max_number - 1);
    assert_eq!(target_active_random_items(max_number, max_number), max_number);
    assert_eq!(target_active_random_items(max_number + 1000, max_number), max_number);
}

#[test]
fn placed_item_spawn_system_spawns_every_placed_item_visible() {
    let mut cells = CellGrid::new(2, 1);
    cells.rows[0][0].has_floor = true;
    cells.rows[0][1].has_floor = true;
    let mut config = map_config(vec![level_grid(cells)], geometry(2, 1));
    config.placed_items = vec![
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 0,
            row: 0,
            item_type: ItemType::Cookie,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 1,
            row: 0,
            item_type: ItemType::Key(BarrierKindId(0)),
        },
    ];

    let mut world = World::new();
    world.insert_resource(config);
    world.insert_resource(geometry(2, 1));
    world.insert_resource(map_settings(true, true));
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(placed_item_spawn_system);
    schedule.run(&mut world);

    let items = world.resource::<ItemMap>();
    assert_eq!(items.iter().count(), 2);
    assert!(items.values().all(|info| !info.is_hidden()));
    let spawned_types: Vec<ItemType> = items.values().map(|info| info.item_type).collect();
    assert!(spawned_types.contains(&ItemType::Cookie));
    assert!(spawned_types.contains(&ItemType::Key(BarrierKindId(0))));
    let mut marker_query = world.query_filtered::<(), With<ItemMarker>>();
    assert_eq!(marker_query.iter(&world).count(), 2);
}

#[test]
fn placed_item_spawn_system_skips_disabled_weapon_pickups() {
    let mut cells = CellGrid::new(3, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    let mut config = map_config(vec![level_grid(cells)], geometry(3, 1));
    config.placed_items = vec![
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 0,
            row: 0,
            item_type: ItemType::MultiShotPowerUp,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 1,
            row: 0,
            item_type: ItemType::MissilePack,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 2,
            row: 0,
            item_type: ItemType::Cookie,
        },
    ];

    let mut world = World::new();
    world.insert_resource(config);
    world.insert_resource(geometry(3, 1));
    world.insert_resource(map_settings(false, false));
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(placed_item_spawn_system);
    schedule.run(&mut world);

    let items = world.resource::<ItemMap>();
    assert_eq!(items.iter().count(), 1);
    assert!(items.values().all(|info| info.item_type == ItemType::Cookie));
}

#[test]
fn choose_item_type_returns_none_for_empty_pool() {
    assert_eq!(choose_item_type(&mut rand::rng(), &[]), None);
}

#[test]
fn choose_item_type_only_picks_pool_members() {
    let pool = [ItemType::SpeedPowerUp, ItemType::Cookie];
    let mut rng = rand::rng();
    for _ in 0..50 {
        let picked = choose_item_type(&mut rng, &pool).expect("non-empty pool must yield an item type");
        assert!(pool.contains(&picked));
    }
}

#[test]
fn random_item_pool_omits_disabled_weapon_pickups() {
    let config = RandomItemsConfig {
        types: vec!["multi_shot".to_owned(), "missile_pack".to_owned(), "cookie".to_owned()],
        max_number: 3,
        despawn_secs: 10.0,
    };
    let random = RandomItems::from_config(Some(&config), map_settings(false, false).weapons);
    assert_eq!(random.pool, vec![ItemType::Cookie]);
}

#[cfg(test)]
mod collection_eligibility_tests {
    use bevy::prelude::*;
    use common::{map::Carriers, protocol::CarrierId};
    use tokio::sync::mpsc::unbounded_channel;

    use crate::{
        config::ServerGameplayConfig,
        items::{ItemInfo, ItemMap, ItemPlacement, item_collection_system},
        network::ServerToClient,
        players::{PlayerInfo, PlayerMap},
        quests::{QuestBoard, QuestCatalog},
    };
    use common::{
        config::GameplayConfig,
        protocol::{
            BarrierKindId, Health, ItemId, ItemMarker, ItemType, PlayerId, PlayerMarker, Position, ServerMessage,
        },
    };

    fn test_app() -> App {
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let gameplay = server.gameplay_config();
        let placed_items = server
            .maps
            .get(&server.default_map)
            .expect("default map settings missing")
            .placed_items
            .clone();
        let quest_catalog = QuestCatalog::from_config(&server);
        let quest_board = QuestBoard::from_catalog(&quest_catalog);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(server)
            .insert_resource(quest_catalog)
            .insert_resource(quest_board)
            .insert_resource(gameplay)
            .insert_resource(placed_items)
            .insert_resource(PlayerMap::default())
            .insert_resource(ItemMap::default())
            .insert_resource(Carriers::default())
            .add_systems(Update, item_collection_system);
        app
    }

    fn spawn_player(
        app: &mut App,
        id: PlayerId,
        pos: Position,
    ) -> (Entity, tokio::sync::mpsc::UnboundedReceiver<ServerToClient>) {
        let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(50.0))).id();
        let (sender, receiver) = unbounded_channel();
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
    fn overlapping_cookie_is_collected_and_scores() {
        let mut app = test_app();
        let id = PlayerId(1);
        let (_, mut rx) = spawn_player(&mut app, id, Position::default());
        let item = spawn_item(&mut app, 1, ItemType::Cookie, Position::default(), random(0.0));

        app.update();

        assert!(
            app.world().resource::<ItemMap>().get(&item).is_none(),
            "cookie consumed"
        );
        let expected = app.world().resource::<ServerGameplayConfig>().scoring.cookie;
        assert_eq!(
            app.world()
                .resource::<PlayerMap>()
                .get(&id)
                .expect("player present")
                .session
                .score,
            expected
        );
        let cookie_cue = std::iter::from_fn(|| rx.try_recv().ok())
            .any(|msg| matches!(msg, ServerToClient::Send(ServerMessage::CookieCollected(_))));
        assert!(cookie_cue, "pickup cue must be unicast");
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
        let item = spawn_item(&mut app, 1, ItemType::Cookie, Position::default(), random(0.0));

        app.update();

        assert!(
            app.world().resource::<ItemMap>().get(&item).is_some(),
            "a same-tick corpse must not vacuum up items"
        );
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
            ItemType::Cookie,
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
        let item = spawn_item(&mut app, 1, ItemType::Cookie, above, random(0.0));

        app.update();

        assert!(
            app.world().resource::<ItemMap>().get(&item).is_some(),
            "vertical epsilon keeps cross-floor pickups out"
        );
    }
}
