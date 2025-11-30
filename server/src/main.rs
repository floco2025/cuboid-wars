use anyhow::Result;
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::net::SocketAddr;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Duration, Instant, MissedTickBehavior},
};

use common::systems::projectiles_system;
use server::{
    config::configure_server,
    net::accept_connections_task,
    resources::{FromAcceptChannel, FromClientsChannel, ItemMap, ItemSpawner, PlayerMap, WallConfig},
    systems::{
        accept_connections_system, broadcast_state_system, hit_detection_system, item_despawn_system,
        item_spawn_system, process_client_message_system, server_movement_system,
    },
    walls::generate_walls,
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

    // Spawn task to accept connections
    tokio::spawn(accept_connections_task(endpoint, to_server_from_accept, to_server));

    // Generate walls
    let wall_config = WallConfig {
        walls: generate_walls(),
    };
    info!("generated {} wall segments", wall_config.walls.len());

    // Create Bevy app with ECS - run in non-blocking mode
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin {
            level: bevy::log::Level::INFO,
            filter: LOG_FILTER.to_string(),
            ..default()
        })
        .insert_resource(PlayerMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(FromAcceptChannel::new(from_accept))
        .insert_resource(FromClientsChannel::new(from_clients))
        .insert_resource(wall_config)
        .add_systems(
            Update,
            (
                // Accept new connections and spawn entities
                accept_connections_system,
                // Makes new entities queryable
                ApplyDeferred,
                // Process messages from clients
                process_client_message_system,
                // Server movement with wall collision
                server_movement_system,
                // Update projectiles (lifetime and despawn)
                projectiles_system,
                // Check for projectile hits
                hit_detection_system,
                // Spawn items
                item_spawn_system,
                // Despawn old items
                item_despawn_system,
                // Broadcast authoritative state to clients
                broadcast_state_system,
            )
                .chain(),
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
