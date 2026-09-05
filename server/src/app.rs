use anyhow::{Result, bail};
use bevy::prelude::*;

use crate::quests::{QuestBoard, QuestCatalog};
use crate::{
    actors::{
        ActorMap, ActorRespawnTimers, ActorSpawner, PendingActorSpawns, actors_plugin,
        navigation::{ActorTerritories, NavGraph},
    },
    characters::characters_plugin,
    combat::{PendingExplosions, combat_plugin},
    config::{ServerGameplayConfig, validate_map_actor_kinds, validate_map_quests},
    items::{ItemMap, ItemSpawner, RandomItems, items_plugin},
    map::{GeneratedMap, LightState, PlateState, WeatherState, generate_map, map_plugin},
    missiles::{AirGraph, MissileMap, missiles_plugin},
    network::{FromClientsChannel, network_plugin},
    players::{Invincibility, PlayerMap, UnlimitedMissiles, players_plugin},
    portals::{PortalAssignments, PortalMap, portals_plugin},
    projectiles::projectiles_plugin,
    schedule::{ServerSet, configure_server_schedule},
};
use common::{
    map::MovingFloors,
    physics::{CollisionWorld, PortalSet},
    protocol::{MapBootstrap, ServerTick, WorldBootstrap, server_tick_advance_system},
};

const LOG_FILTER: &str = "wgpu=error,naga=warn";

pub fn build_server_app(map_override: Option<&str>, from_clients: FromClientsChannel) -> Result<App> {
    let server_gameplay_config = ServerGameplayConfig::load_default()?;
    let gameplay_config = server_gameplay_config.gameplay_config();
    let map_name = map_override.unwrap_or(&server_gameplay_config.default_map);
    let Some(map_server_config) = server_gameplay_config.maps.get(map_name).cloned() else {
        let mut known: Vec<&str> = server_gameplay_config.maps.keys().map(String::as_str).collect();
        known.sort_unstable();
        bail!("unknown map {map_name:?} (available: {known:?})");
    };
    let map_settings = map_server_config.settings.clone();
    let placed_items_config = map_server_config.placed_items.clone();
    let weather_state = WeatherState::new(server_gameplay_config.cycles.weather.clone(), map_server_config.weather);
    let light_state = LightState::new(
        server_gameplay_config.cycles.lighting.clone(),
        map_server_config.lighting,
    );
    let random_items = RandomItems::from_config(map_server_config.random_items.as_ref(), map_settings.weapons);
    let portal_assignments = PortalAssignments::new(map_settings.weapons.portals);
    let (barrier_kind_table, bridge_kind_table) = map_settings.kind_tables()?;
    let GeneratedMap {
        layout: map_layout,
        config: map_config,
        geometry: map_geometry,
    } = generate_map(map_name, map_settings.geometry, &barrier_kind_table, &bridge_kind_table)?;
    let collision_world = CollisionWorld::from_map_layout(&map_layout, &barrier_kind_table);
    let moving_floors = MovingFloors::from_layout(&map_layout);
    let nav_graph = NavGraph::new(map_config.clone(), map_geometry);
    let air_graph = AirGraph::new(map_config.clone(), map_geometry);
    validate_map_actor_kinds(&server_gameplay_config, &map_config)?;
    validate_map_quests(
        &map_server_config.quests,
        &map_config,
        map_server_config.random_items.as_ref(),
    )?;
    let quest_catalog = QuestCatalog::from_quests(&map_server_config.quests);
    let quest_board = QuestBoard::from_catalog(&quest_catalog);
    let actor_territories = ActorTerritories::new(&nav_graph, &map_config, &server_gameplay_config)?;
    let world_bootstrap = WorldBootstrap {
        gameplay: server_gameplay_config.gameplay_bootstrap(),
        map: MapBootstrap {
            layout: map_layout.clone(),
            settings: map_settings.clone(),
            key_kinds: map_config.key_kinds(),
        },
    };

    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::INFO,
        filter: LOG_FILTER.to_string(),
        ..default()
    });

    info!("generated map {map_name:?}: {}", map_layout.summary());

    app.insert_resource(map_layout)
        .insert_resource(map_settings)
        .insert_resource(world_bootstrap)
        .insert_resource(weather_state)
        .insert_resource(light_state)
        .insert_resource(Invincibility(false))
        .insert_resource(UnlimitedMissiles(false))
        .insert_resource(collision_world)
        .insert_resource(moving_floors)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(nav_graph)
        .insert_resource(actor_territories)
        .insert_resource(air_graph)
        .insert_resource(barrier_kind_table)
        .insert_resource(bridge_kind_table)
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(quest_catalog)
        .insert_resource(quest_board)
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(random_items)
        .insert_resource(placed_items_config)
        .insert_resource(ActorSpawner::default())
        .insert_resource(ActorRespawnTimers::default())
        .insert_resource(PendingActorSpawns::default())
        .insert_resource(from_clients)
        .insert_resource(PendingExplosions::default())
        .insert_resource(MissileMap::default())
        .insert_resource(PortalMap::default())
        .insert_resource(portal_assignments)
        .insert_resource(PortalSet::default())
        .insert_resource(PlateState::default())
        .insert_resource(ServerTick::default());

    configure_server_schedule(&mut app);
    app.add_systems(Update, server_tick_advance_system.in_set(ServerSet::Prepare));
    app.add_plugins((
        actors_plugin,
        characters_plugin,
        combat_plugin,
        items_plugin,
        map_plugin,
        missiles_plugin,
        network_plugin,
        players_plugin,
        portals_plugin,
        projectiles_plugin,
    ));

    Ok(app)
}
