use std::{thread, time::Instant};

use anyhow::{Error, Result};
use bevy::prelude::*;

use crate::{
    actors::{
        ActorMap, ActorSpawner, PendingActorSpawns, actors_plugin,
        navigation::{ActorTerritories, surface::SurfaceNavigation},
    },
    characters::characters_plugin,
    combat::{PendingExplosions, combat_plugin},
    config::{FallDamageConfigs, GameplayCatalog, ServerGameplayConfig, validate_map_actor_kinds, validate_map_quests},
    items::{ItemMap, ItemSpawner, RandomItems, items_plugin},
    map::{GeneratedMap, MapFireworks, Switches, WeatherState, generate_map, map_plugin},
    missiles::{MissileMap, missiles_plugin},
    network::{ClientLinks, Listener, LocalLink, network_plugin, register_local},
    players::{Invincibility, LoginStart, PlayerMap, checkpoint_numbered, players_plugin},
    portals::{PortalAssignments, PortalMap, portals_plugin},
    projectiles::projectiles_plugin,
    quests::{QuestBoard, QuestCatalog},
    schedule::{ServerSet, configure_server_schedule},
};
use bevy::time::TimeUpdateStrategy;
use common::{
    celestial::CelestialClockAnchor,
    config::NetworkConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{
        MapBootstrap, MapSettings, MissileAirGrid, Position, ServerTick, WorldBootstrap, server_tick_advance_system,
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

pub struct ServerAppOptions {
    pub map: Option<String>,
    pub god: bool,
    pub peace: bool,
    pub initial_spawn: Option<Position>,
    // The number of the checkpoint every login starts at.
    pub checkpoint: Option<u32>,
    pub network: NetworkOverrides,
    // Only one Bevy `LogPlugin` may install per process; the app built first owns it.
    pub logging: bool,
}

// `listener` is the UDP endpoint remote clients join through and `local` the
// host's own client; a dedicated server has no local client, single-player
// no listener.
pub fn build_server_app(
    options: ServerAppOptions,
    listener: Option<Listener>,
    local: Option<LocalLink>,
) -> Result<App> {
    build_server_app_with_loader(
        GameplayCatalog::load_default()?.select(options.map.as_deref())?,
        options,
        listener,
        local,
        generate_map,
    )
}

fn build_server_app_with_loader(
    mut server_gameplay_config: ServerGameplayConfig,
    options: ServerAppOptions,
    listener: Option<Listener>,
    local: Option<LocalLink>,
    load_map: impl FnOnce(&str, u32, &MapSettings) -> Result<GeneratedMap>,
) -> Result<App> {
    options.network.apply(&mut server_gameplay_config.network);
    server_gameplay_config.network.validate()?;
    let gameplay_config = server_gameplay_config.gameplay_config();
    let map_name = server_gameplay_config.map_name.clone();
    let GeneratedMap {
        layout: map_layout,
        config: map_config,
        settings: map_settings,
        fields: field_table,
        switch_table,
        fireworks,
        fireworks_switch,
    } = load_map(
        &map_name,
        server_gameplay_config.network.server_hz,
        &server_gameplay_config.settings,
    )?;
    let power_ups_config = server_gameplay_config.power_ups.clone();
    let placed_items_config = server_gameplay_config.placed_items.clone().unwrap_or_default();
    let weather_state = WeatherState::new(
        server_gameplay_config.cycles.weather.clone(),
        server_gameplay_config.weather,
    );
    let celestial_clock = CelestialClockAnchor::initial(&map_settings.celestial, 0);
    let random_items = RandomItems::from_config(server_gameplay_config.random_items.as_ref());
    let portal_assignments = PortalAssignments::new(map_settings.portals);
    let map_geometry = map_config.root_grid().geometry;
    for (index, item) in map_config.placed_items.iter().enumerate() {
        power_ups_config.validate_pickup(item.item_type, &format!("map {map_name} placed_items[{index}]"))?;
    }
    let map_items = map_config.available_items(random_items.pool.iter().map(|&(item_type, _)| item_type));
    let mut collision_world = CollisionWorld::from_map_layout(&map_layout);
    let carriers = Carriers::from_layout(&map_layout);
    validate_map_actor_kinds(&server_gameplay_config, &map_config)?;
    validate_map_quests(
        &server_gameplay_config.quests,
        &map_config,
        server_gameplay_config.random_items.as_ref(),
        fireworks_switch,
    )?;
    let quest_catalog = QuestCatalog::from_quests(&server_gameplay_config.quests);
    let quest_board = QuestBoard::from_catalog(&quest_catalog, fireworks_switch);
    collision_world.set_locked_pressure_plates(quest_board.locked_switches());
    let switches = Switches::new(
        &map_settings,
        &switch_table,
        &map_layout,
        fireworks.as_ref(),
        server_gameplay_config.network.server_hz,
    );
    let switch_state = switches.state();
    let surface_navigation = SurfaceNavigation::build(
        &map_config,
        &map_layout,
        &server_gameplay_config,
        &collision_world,
        &switch_state.open_fields,
        quest_board.locked_switches(),
    )?;
    let actor_territories = ActorTerritories::new(&map_config, &server_gameplay_config);
    let world_bootstrap = WorldBootstrap {
        network: server_gameplay_config.network,
        celestial: server_gameplay_config.cycles.celestial,
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

    let login_start = LoginStart {
        spawn: options.initial_spawn,
        checkpoint: options
            .checkpoint
            .map(|number| checkpoint_numbered(&map_layout.checkpoints, number))
            .transpose()
            .map_err(Error::msg)?,
    };

    let respawn = server_gameplay_config.respawn;
    let mut app = App::new();
    // Server time is tick time: every update advances `Time` by exactly one
    // tick, so delta-driven timers and tick-driven carriers agree and the
    // integration matches the client's fixed step. An overrun skips wall
    // time (`run_server_loop`) instead of stretching a tick.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        server_gameplay_config.network.tick_duration(),
    ));
    app.add_plugins(MinimalPlugins);
    if options.logging {
        app.add_plugins(bevy::log::LogPlugin {
            level: bevy::log::Level::INFO,
            filter: LOG_FILTER.to_string(),
            ..default()
        });
    }

    info!("generated map {map_name:?}: {}", map_layout.summary());

    let mut actors = ActorMap::default();
    actors.set_peaceful(options.peace);
    app.insert_resource(server_gameplay_config.network)
        .insert_resource(map_layout)
        .insert_resource(map_items)
        .insert_resource(map_settings)
        .insert_resource(FallDamageConfigs {
            player: server_gameplay_config.player_fall,
            actor: server_gameplay_config.actor_fall,
        })
        .insert_resource(world_bootstrap)
        .insert_resource(weather_state)
        .insert_resource(celestial_clock)
        .insert_resource(Invincibility(options.god))
        .insert_resource(login_start)
        .insert_resource(collision_world)
        .insert_resource(carriers)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(surface_navigation)
        .insert_resource(actor_territories)
        .insert_resource(field_table)
        .insert_resource(switch_table)
        .insert_resource(MapFireworks(fireworks))
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(quest_catalog)
        .insert_resource(quest_board)
        .insert_resource(PlayerMap::new(respawn, power_ups_config.always_active()))
        .insert_resource(actors)
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(random_items)
        .insert_resource(placed_items_config)
        .insert_resource(power_ups_config)
        .insert_resource(ActorSpawner::default())
        .insert_resource(PendingActorSpawns::default())
        .insert_resource(ClientLinks::default())
        .insert_resource(ServerTick::default());
    if let Some(listener) = listener {
        app.insert_resource(listener);
    }
    app.insert_resource(PendingExplosions::default())
        .insert_resource(MissileMap::default())
        .insert_resource(PortalMap::default())
        .insert_resource(portal_assignments)
        .insert_resource(switches)
        .insert_resource(switch_state);

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
    if let Some(link) = local {
        register_local(app.world_mut(), link);
    }

    Ok(app)
}

// Paces the ticks on absolute wall-clock deadlines; after an overrun the
// missed ticks are skipped rather than caught up. Bevy's `ScheduleRunnerPlugin`
// would instead sleep for what is left of the tick after each update, so every
// sleep overshoot would accumulate as tick-rate drift, and server time is tick
// time.
pub fn run_server_loop(mut app: App) -> ! {
    info!("starting ECS server loop...");
    let tick_duration = app.world().resource::<NetworkConfig>().tick_duration();
    let mut next_tick = Instant::now();
    let mut frame: u64 = 0;
    loop {
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
        } else if now - next_tick > tick_duration {
            next_tick = now;
        }
        next_tick += tick_duration;

        let update_start = Instant::now();
        app.update();
        let update_elapsed = update_start.elapsed();

        if update_elapsed > tick_duration {
            warn!(
                "tick {} took {:.2}ms (exceeded {:.2}ms budget)",
                frame,
                update_elapsed.as_secs_f64() * 1000.0,
                tick_duration.as_secs_f64() * 1000.0
            );
        }

        frame += 1;
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/app_rate.rs"]
mod rate_tests;

#[cfg(test)]
#[path = "tests/app_surface.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "tests/app_pursuit.rs"]
mod pursuit_tests;

#[cfg(test)]
#[path = "tests/app_home.rs"]
mod home_tests;

#[cfg(test)]
#[path = "tests/app_roaming.rs"]
mod roaming_tests;

#[cfg(test)]
#[path = "tests/app_actor_collisions.rs"]
mod actor_collision_tests;

#[cfg(test)]
#[path = "tests/app_fixtures.rs"]
pub(crate) mod fixtures;
