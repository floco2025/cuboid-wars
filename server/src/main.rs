use anyhow::{Context, Result, bail};
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::{collections::HashSet, net::SocketAddr};
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Duration, Instant, MissedTickBehavior},
};

use common::{config::GameplayConfig, constants::TICK_HZ, physics::CollisionWorld};
use server::{
    actors::{
        actor_behavior_system, actor_initial_spawn_system, actor_removal_system, actor_respawn_system,
        navigation::NavGraph,
    },
    characters::{characters_health_regeneration_system, characters_movement_system},
    config::{ServerGameplayConfig, configure_server},
    items::{
        item_collection_system, item_despawn_system, item_initial_spawn_system, item_respawn_system, item_spawn_system,
        key_initial_spawn_system,
    },
    map::generate_map,
    net::accept_connections_task,
    network::{
        network_accept_connections_system, network_broadcast_snapshot_system, network_process_client_messages_system,
    },
    players::{
        players_fall_damage_system, players_fall_death_system, players_respawn_system, players_status_timers_system,
    },
    projectiles::projectiles_movement_system,
    resources::*,
};

const LOG_FILTER: &str = "wgpu=error,naga=warn";

// ============================================================================
// CLI Argument Parsing
// ============================================================================

#[derive(Parser)]
#[command(author, version, about = "Cuboid Wars Server", long_about = None)]
struct Args {
    // Address to bind server to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    bind: String,
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let addr: SocketAddr = args.bind.parse()?;
    let server_config = configure_server()?;
    let gameplay_config = GameplayConfig::load_default()?;
    let server_gameplay_config = ServerGameplayConfig::load_default()?;
    let endpoint = Endpoint::server(server_config, addr)?;
    println!("quic server listening on {addr}");

    let barrier_kind_table = common::protocol::BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .with_context(|| "failed to build BarrierKindTable from gameplay.json barrier_kinds")?;
    let (map_layout, map_config, map_geometry) = generate_map(&barrier_kind_table);
    let collision_world = CollisionWorld::from_map_layout(&map_layout, &barrier_kind_table);
    let nav_graph = NavGraph::new(map_config.clone(), map_geometry);
    validate_actor_kinds_consistent(&gameplay_config, &server_gameplay_config, &map_config)?;

    // Channel for sending from the accept connections task to the server
    let (to_server_from_accept, from_accept) = unbounded_channel();
    // Channel for sending from all per client network IO tasks to the server
    let (to_server, from_clients) = unbounded_channel();

    tokio::spawn(accept_connections_task(endpoint, to_server_from_accept, to_server));
    let mut app = App::new();

    app.add_plugins(MinimalPlugins).add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::INFO,
        filter: LOG_FILTER.to_string(),
        ..default()
    });

    info!(
        "generated map: {} walls, {} ramps, {} floors",
        map_layout.walls.len(),
        map_layout.ramps.len(),
        map_layout.floors.len(),
    );

    app.insert_resource(map_layout)
        .insert_resource(collision_world)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(nav_graph)
        .insert_resource(barrier_kind_table)
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(ActorSpawner::default())
        .insert_resource(ActorSpawnThrottles::default())
        .insert_resource(FromAcceptChannel::new(from_accept))
        .insert_resource(FromClientsChannel::new(from_clients))
        .add_systems(Startup, actor_initial_spawn_system)
        .add_systems(
            Update,
            (
                // Network systems must run in order:
                // 1. Accept new connections (spawns entities)
                // 2. ApplyDeferred (makes entities queryable)
                // 3. Process client messages (needs to query those entities)
                // 4. ApplyDeferred (makes message-side component changes queryable)
                // 5. Broadcast state to all clients
                (
                    network_accept_connections_system,
                    ApplyDeferred,
                    network_process_client_messages_system,
                    ApplyDeferred,
                    network_broadcast_snapshot_system,
                )
                    .chain(),
                // Actor decisions are written before movement consumes them.
                actor_behavior_system,
                characters_movement_system.after(actor_behavior_system),
                projectiles_movement_system,
                // Actor removal handles health-zero explosions and fall cleanup.
                actor_removal_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system)
                    .before(characters_health_regeneration_system),
                characters_health_regeneration_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system),
                // Fall recovery must run after movement updates positions.
                players_fall_death_system.after(characters_movement_system),
                // Fall damage observes post-step vy, so must follow movement
                // and precede fall_death (a lethal fall-damage hit and a
                // void-fall on the same tick should resolve via the impact
                // path, not the void path).
                players_fall_damage_system
                    .after(characters_movement_system)
                    .before(players_fall_death_system),
                actor_respawn_system,
                players_respawn_system,
                players_status_timers_system,
                item_initial_spawn_system,
                key_initial_spawn_system,
                item_spawn_system,
                item_despawn_system,
                item_collection_system,
                item_respawn_system,
            ),
        );

    info!("starting ECS server loop...");

    // Run the app in a loop manually at LOOP_FREQUENCY Hz
    let tick_duration = Duration::from_nanos(1_000_000_000 / u64::from(TICK_HZ));
    let mut interval = time::interval(tick_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut frame: u64 = 0;
    loop {
        interval.tick().await;

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

// Cross-file consistency check: every actor kind referenced by the map must
// be defined in both the common and server gameplay configs, and the two
// config files must define the same set of kinds. The map validator does not
// know about gameplay configs, so this check lives at startup where all
// three are paired together.
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
