use anyhow::{Result, bail};
use bevy::prelude::*;

use crate::{
    actors::{
        ActorMap, ActorRespawnTimers, ActorSpawner, PendingActorSpawns, actors_plugin,
        navigation::{ActorTerritories, NavGraphs},
    },
    characters::characters_plugin,
    combat::{PendingExplosions, combat_plugin},
    config::{ServerGameplayConfig, validate_map_actor_kinds, validate_map_quests},
    items::{ItemMap, ItemSpawner, RandomItems, items_plugin},
    map::{GeneratedMap, LightState, MapFireworks, WeatherState, generate_map, map_plugin},
    missiles::{MissileMap, missiles_plugin},
    network::{FromClientsChannel, network_plugin},
    players::{Invincibility, PlayerMap, players_plugin},
    portals::{PortalAssignments, PortalMap, portals_plugin},
    projectiles::projectiles_plugin,
    quests::{QuestBoard, QuestCatalog},
    schedule::{ServerSet, configure_server_schedule},
};
use bevy::time::TimeUpdateStrategy;
use common::{
    config::NetworkConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{
        MapBootstrap, MapSettings, MissileAirGrid, PlateState, ServerTick, WorldBootstrap, server_tick_advance_system,
    },
};

const LOG_FILTER: &str = "wgpu=error,naga=warn";

// Command-line rate overrides; each one replaces its `gameplay.json::network` field.
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkOverrides {
    pub server_hz: Option<u32>,
    pub update_hz: Option<u32>,
    pub snapshot_hz: Option<u32>,
}

impl NetworkOverrides {
    fn apply(self, network: &mut NetworkConfig) {
        if let Some(server_hz) = self.server_hz {
            network.server_hz = server_hz;
        }
        if let Some(update_hz) = self.update_hz {
            network.update_hz = update_hz;
        }
        if let Some(snapshot_hz) = self.snapshot_hz {
            network.snapshot_hz = snapshot_hz;
        }
    }
}

pub fn build_server_app(
    map_override: Option<&str>,
    overrides: NetworkOverrides,
    from_clients: FromClientsChannel,
) -> Result<App> {
    build_server_app_with_loader(
        ServerGameplayConfig::load_default()?,
        map_override,
        overrides,
        from_clients,
        generate_map,
    )
}

fn build_server_app_with_loader(
    mut server_gameplay_config: ServerGameplayConfig,
    map_override: Option<&str>,
    overrides: NetworkOverrides,
    from_clients: FromClientsChannel,
    load_map: impl FnOnce(&str, u32, &MapSettings) -> Result<GeneratedMap>,
) -> Result<App> {
    overrides.apply(&mut server_gameplay_config.network);
    server_gameplay_config.network.validate()?;
    let gameplay_config = server_gameplay_config.gameplay_config();
    let map_name = map_override.unwrap_or(&server_gameplay_config.default_map);
    let Some(map_server_config) = server_gameplay_config.maps.get(map_name).cloned() else {
        let mut known: Vec<&str> = server_gameplay_config.maps.keys().map(String::as_str).collect();
        known.sort_unstable();
        bail!("unknown map {map_name:?} (available: {known:?})");
    };
    let GeneratedMap {
        layout: map_layout,
        config: map_config,
        settings: map_settings,
        barrier_kinds: barrier_kind_table,
        bridge_kinds: bridge_kind_table,
        switch_table,
        fireworks,
        fireworks_switch,
    } = load_map(
        map_name,
        server_gameplay_config.network.server_hz,
        &map_server_config.settings,
    )?;
    let power_ups_config = map_server_config.power_ups.clone();
    let placed_items_config = map_server_config.placed_items.clone();
    let weather_state = WeatherState::new(server_gameplay_config.cycles.weather.clone(), map_server_config.weather);
    let light_state = LightState::new(
        server_gameplay_config.cycles.lighting.clone(),
        map_server_config.lighting,
    );
    let random_items = RandomItems::from_config(map_server_config.random_items.as_ref());
    let portal_assignments = PortalAssignments::new(map_settings.portals);
    let map_geometry = map_config.root_grid().geometry;
    let map_items = map_config.available_items(&random_items.pool);
    let collision_world = CollisionWorld::from_map_layout(&map_layout);
    let carriers = Carriers::from_layout(&map_layout);
    let mut nav_graphs = NavGraphs::new(&map_config);
    nav_graphs.add_ladder_routes(&map_layout, &map_settings, &server_gameplay_config);
    validate_map_actor_kinds(&server_gameplay_config, &map_config)?;
    validate_map_quests(
        &map_server_config.quests,
        &map_config,
        map_server_config.random_items.as_ref(),
        fireworks_switch,
    )?;
    let quest_catalog = QuestCatalog::from_quests(&map_server_config.quests);
    let quest_board = QuestBoard::from_catalog(&quest_catalog, fireworks_switch);
    let actor_territories = ActorTerritories::new(&nav_graphs, &map_config, &server_gameplay_config)?;
    let world_bootstrap = WorldBootstrap {
        network: server_gameplay_config.network,
        gameplay: server_gameplay_config.gameplay_bootstrap(),
        map: MapBootstrap {
            layout: map_layout.clone(),
            settings: map_settings.clone(),
            items: map_items.clone(),
            missile_air_grids: map_config
                .grids
                .iter()
                .map(|grid| MissileAirGrid {
                    carrier: grid.carrier,
                    cols: grid.geometry.grid_cols,
                    rows: grid.geometry.grid_rows,
                    levels: u8::try_from(grid.levels.len()).expect("map level count exceeds u8"),
                })
                .collect(),
        },
    };

    let mut app = App::new();
    // Server time is tick time: every update advances `Time` by exactly one
    // tick, so delta-driven timers and tick-driven carriers agree and the
    // integration matches the client's fixed step. An overrun skips wall
    // time (`MissedTickBehavior::Skip` in main.rs) instead of stretching a
    // tick.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        server_gameplay_config.network.tick_duration(),
    ));
    app.add_plugins(MinimalPlugins).add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::INFO,
        filter: LOG_FILTER.to_string(),
        ..default()
    });

    info!("generated map {map_name:?}: {}", map_layout.summary());

    app.insert_resource(server_gameplay_config.network)
        .insert_resource(map_layout)
        .insert_resource(map_items)
        .insert_resource(map_settings)
        .insert_resource(map_server_config.player_fall)
        .insert_resource(world_bootstrap)
        .insert_resource(weather_state)
        .insert_resource(light_state)
        .insert_resource(Invincibility(false))
        .insert_resource(collision_world)
        .insert_resource(carriers)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(nav_graphs)
        .insert_resource(actor_territories)
        .insert_resource(barrier_kind_table)
        .insert_resource(bridge_kind_table)
        .insert_resource(switch_table)
        .insert_resource(MapFireworks(fireworks))
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(quest_catalog)
        .insert_resource(quest_board)
        .insert_resource(PlayerMap::new(map_server_config.respawn))
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(random_items)
        .insert_resource(placed_items_config)
        .insert_resource(power_ups_config)
        .insert_resource(ActorSpawner::default())
        .insert_resource(ActorRespawnTimers::default())
        .insert_resource(PendingActorSpawns::default())
        .insert_resource(from_clients)
        .insert_resource(PendingExplosions::default())
        .insert_resource(MissileMap::default())
        .insert_resource(PortalMap::default())
        .insert_resource(portal_assignments)
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

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/app_rate.rs"]
mod rate_tests;

#[cfg(test)]
#[path = "tests/app_fixtures.rs"]
mod fixtures;
