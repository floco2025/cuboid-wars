use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use bevy::prelude::*;

use crate::{
    actors::{ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawns, actors_plugin, navigation::NavGraph},
    characters::characters_plugin,
    combat::{PendingExplosions, combat_plugin},
    config::ServerGameplayConfig,
    items::{ItemMap, ItemSpawner, RandomItems, items_plugin},
    map::{CurrentLighting, MapConfig, OpenBarrierKinds, WeatherState, generate_map, map_plugin},
    missiles::{AirGraph, MissileMap, missiles_plugin},
    network::{FromClientsChannel, network_plugin},
    players::{Invincibility, PlayerMap, UnlimitedMissiles, players_plugin},
    projectiles::projectiles_plugin,
    schedule::configure_server_schedule,
};
use common::{config::GameplayConfig, physics::CollisionWorld, protocol::BarrierKindTable};

const LOG_FILTER: &str = "wgpu=error,naga=warn";

pub fn build_server_app(map_override: Option<&str>, from_clients: FromClientsChannel) -> Result<App> {
    let gameplay_config = GameplayConfig::load_default()?;
    let server_gameplay_config = ServerGameplayConfig::load_default()?;
    let map_name = map_override.unwrap_or(&server_gameplay_config.default_map);
    let Some(map_server_config) = server_gameplay_config.maps.get(map_name).cloned() else {
        let mut known: Vec<&str> = server_gameplay_config.maps.keys().map(String::as_str).collect();
        known.sort_unstable();
        bail!("unknown map {map_name:?} (available: {known:?})");
    };
    let map_settings = map_server_config.settings.clone();
    let weather_state = WeatherState::new(map_server_config.rain.clone());
    let random_items = RandomItems::from_config(map_server_config.random_items.as_ref());

    let barrier_kind_table = BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .with_context(|| "failed to build BarrierKindTable from gameplay.json barrier_kinds")?;
    let (map_layout, map_config, map_geometry) = generate_map(&barrier_kind_table, map_name);
    let collision_world = CollisionWorld::from_map_layout(&map_layout, &barrier_kind_table);
    let nav_graph = NavGraph::new(map_config.clone(), map_geometry);
    let air_graph = AirGraph::new(map_config.clone(), map_geometry);
    validate_actor_kinds_consistent(&gameplay_config, &server_gameplay_config, &map_config)?;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::INFO,
        filter: LOG_FILTER.to_string(),
        ..default()
    });

    info!(
        "generated map {map_name:?}: {} walls, {} ramps, {} floors",
        map_layout.walls.len(),
        map_layout.ramps.len(),
        map_layout.floors.len(),
    );

    app.insert_resource(map_layout)
        .insert_resource(map_settings)
        .insert_resource(weather_state)
        .init_resource::<CurrentLighting>()
        .insert_resource(Invincibility(false))
        .insert_resource(UnlimitedMissiles(false))
        .insert_resource(collision_world)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(nav_graph)
        .insert_resource(air_graph)
        .insert_resource(barrier_kind_table)
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(random_items)
        .insert_resource(ActorSpawner::default())
        .insert_resource(ActorSpawnThrottles::default())
        .insert_resource(PendingActorSpawns::default())
        .insert_resource(from_clients)
        .insert_resource(PendingExplosions::default())
        .insert_resource(MissileMap::default())
        .insert_resource(OpenBarrierKinds::default());

    configure_server_schedule(&mut app);
    app.add_plugins((
        actors_plugin,
        characters_plugin,
        combat_plugin,
        items_plugin,
        map_plugin,
        missiles_plugin,
        network_plugin,
        players_plugin,
        projectiles_plugin,
    ));

    Ok(app)
}

fn validate_actor_kinds_consistent(
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    map_config: &MapConfig,
) -> Result<()> {
    let common_kinds: HashSet<&str> = gameplay_config.actors.keys().map(String::as_str).collect();
    let server_kinds: HashSet<&str> = server_gameplay_config.actors.keys().map(String::as_str).collect();
    if common_kinds != server_kinds {
        let only_common: Vec<&str> = common_kinds.difference(&server_kinds).copied().collect();
        let only_server: Vec<&str> = server_kinds.difference(&common_kinds).copied().collect();
        bail!(
            "actor kinds disagree between common and server gameplay configs (only in common: {only_common:?}, only in server: {only_server:?})"
        );
    }
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if !common_kinds.contains(zone.kind.as_str()) {
            bail!(
                "map actor spawn zone {zone_idx} references unknown actor kind {:?} (known kinds: {:?})",
                zone.kind,
                common_kinds
            );
        }
    }
    Ok(())
}
