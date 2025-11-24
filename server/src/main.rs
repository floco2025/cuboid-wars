use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::net::SocketAddr;
use tokio::sync::mpsc::unbounded_channel;

use common::protocol::PlayerId;
use server::{
    config::configure_server,
    net::{ClientToServer, per_client_network_io_task},
    resources::{ClientsToServerChannel, PendingMessages, PlayerIndex},
    systems::{handle_network_connections_system, process_messages_system},
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

    // Channel for sending from all per client network IO tasks to the server
    let (to_server, from_clients) = unbounded_channel();
    let to_server_clone = to_server.clone();

    // Spawn task to accept connections and handle them
    tokio::spawn(async move {
        let mut next_player_id = 1u32;
        loop {
            if let Some(incoming) = endpoint.accept().await {
                // Generate ID
                let id = PlayerId(next_player_id);
                next_player_id = next_player_id
                    .checked_add(1)
                    .expect("player ID overflow: 4 billion players connected!");

                let to_server_clone = to_server_clone.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(connection) => {
                            info!("player {:?} connection established", id);

                            // Channel for sending from the server to a new client network IO task
                            let (to_client, from_server) = unbounded_channel();

                            // Notify main thread to spawn entity
                            if to_server_clone
                                .send((id, ClientToServer::Connected(to_client)))
                                .is_err()
                            {
                                error!("failed to send Connected event for {:?}", id);
                                return;
                            }

                            // Run per-client network I/O task
                            per_client_network_io_task(id, connection, to_server_clone, from_server).await;
                        }
                        Err(e) => {
                            error!("failed to establish connection: {}", e);
                        }
                    }
                });
            }
        }
    });

    // Create Bevy app with ECS - run in non-blocking mode
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin {
            level: bevy::log::Level::INFO,
            filter: "wgpu=error,naga=warn".to_string(),
            ..default()
        })
        .insert_resource(PlayerIndex::default())
        .insert_resource(PendingMessages::default())
        .insert_resource(ClientsToServerChannel(from_clients))
        .add_systems(
            Update,
            (
                handle_network_connections_system,
                ApplyDeferred,
                process_messages_system,
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
