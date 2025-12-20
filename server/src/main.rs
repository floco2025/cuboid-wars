use anyhow::Result;
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::net::SocketAddr;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Duration, Instant, MissedTickBehavior},
};

use server::{
    config::configure_server,
    constants::GHOSTS_NUM,
    map::generate_grid,
    net::accept_connections_task,
    resources::*,
    systems::{ghosts::*, items::*, network::*, players::*, projectiles::*},
};

const SERVER_LOOP_FREQUENCY: u64 = 30;
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

    // Disable ghost spawning
    #[arg(long, default_value_t = false)]
    no_ghosts: bool,
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let addr: SocketAddr = args.bind.parse()?;
    let server_config = configure_server()?;
    let endpoint = Endpoint::server(server_config, addr)?;
    println!("quic server listening on {addr}");

    // Channel for sending from the accept connections task to the server
    let (to_server_from_accept, from_accept) = unbounded_channel();
    // Channel for sending from all per client network IO tasks to the server
    let (to_server, from_clients) = unbounded_channel();

    tokio::spawn(accept_connections_task(endpoint, to_server_from_accept, to_server));
    let mut app = App::new();

    let (map_layout, grid_config) = generate_grid();
    info!(
        "generated {} wall segments, {} roofs, {} ramps",
        map_layout.lower_walls.len(),
        map_layout.roofs.len(),
        map_layout.ramps.len()
    );

    let ghost_spawn_config = GhostSpawnConfig {
        num_ghosts: if args.no_ghosts { 0 } else { GHOSTS_NUM },
    };

    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin {
            level: bevy::log::Level::INFO,
            filter: LOG_FILTER.to_string(),
            ..default()
        })
        .insert_resource(map_layout)
        .insert_resource(grid_config)
        .insert_resource(ghost_spawn_config)
        .insert_resource(PlayerMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(GhostMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(FromAcceptChannel::new(from_accept))
        .insert_resource(FromClientsChannel::new(from_clients))
        .add_systems(
            Update,
            (
                // Network systems must run in order:
                // 1. Accept new connections (spawns entities)
                // 2. ApplyDeferred (makes entities queryable)
                // 3. Process client messages (needs to query those entities)
                // 4. Broadcast state to all clients
                (
                    network_accept_connections_system,
                    ApplyDeferred,
                    network_client_message_system,
                    network_broadcast_state_system,
                )
                    .chain(),
                // Game logic systems can run in parallel
                players_movement_system,
                players_timer_system,
                ghosts_spawn_system,
                ghosts_movement_system,
                ghost_player_collision_system,
                projectiles_movement_system,
                item_initial_spawn_system,
                item_spawn_system,
                item_despawn_system,
                item_collection_system,
                item_respawn_system,
            ),
        );

    info!("starting ECS server loop...");

    // Run the app in a loop manually at LOOP_FREQUENCY Hz
    let tick_duration = Duration::from_nanos(1_000_000_000 / SERVER_LOOP_FREQUENCY);
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
