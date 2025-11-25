use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::net::SocketAddr;
use tokio::sync::mpsc::unbounded_channel;

use server::{
    config::configure_server,
    net::accept_connections_task,
    resources::{FromAcceptChannel, FromClientsChannel, PlayerIndex},
    systems::{process_client_message_system, process_new_connections_system},
};

// ============================================================================
// CLI Argument Parsing
// ============================================================================

#[derive(Parser)]
#[command(author, version, about = "Game server", long_about = None)]
struct Args {
    /// Address to bind server to
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
    info!("quic server listening on {}", addr);

    // Channel for sending from the accept connections task to the server
    let (to_server_from_accept, from_accept) = unbounded_channel();
    // Channel for sending from all per client network IO tasks to the server
    let (to_server, from_clients) = unbounded_channel();

    // Spawn task to accept connections
    tokio::spawn(accept_connections_task(endpoint, to_server_from_accept, to_server));

    // Create Bevy app with ECS - run in non-blocking mode
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin {
            level: bevy::log::Level::INFO,
            filter: "wgpu=error,naga=warn".to_string(),
            ..default()
        })
        .insert_resource(PlayerIndex::default())
        .insert_resource(FromAcceptChannel::new(from_accept))
        .insert_resource(FromClientsChannel::new(from_clients))
        .add_systems(
            Update,
            (
                process_new_connections_system, // Spawns entities
                ApplyDeferred,                  // Makes them queryable
                process_client_message_system,  // Can now query them
            )
                .chain(),
        );

    info!("starting ECS server loop...");

    // Run the app in a loop manually at 30 Hz
    let tick_duration = tokio::time::Duration::from_nanos(1_000_000_000 / 30);
    let mut interval = tokio::time::interval(tick_duration);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut frame: u64 = 0;
    loop {
        interval.tick().await;

        let update_start = tokio::time::Instant::now();
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
        // if frame % 30 == 0 {
        //     trace!("server tick {}", frame);
        // }
    }
}
