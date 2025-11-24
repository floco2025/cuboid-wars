use anyhow::{Context, Result};
use bevy::prelude::*;
use clap::Parser;
use client::client::ClientState;
use client::config::configure_client;
use client::net::{network_io_task, BevyToServerChannel, ServerToBevyChannel, server_to_bevy_system};
use client::sync::{sync_players, update_player_positions};
use client::world::setup_world;
use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::Endpoint;

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(author, version, about = "Game client", long_about = None)]
struct Args {
    /// Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    let args = Args::parse();

    // Create tokio runtime for network I/O
    let rt = tokio::runtime::Runtime::new()?;

    // Connect to server (blocking)
    println!("Connecting to server...");
    let connection = rt.block_on(async {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        let client_config = configure_client()?;
        endpoint.set_default_client_config(client_config);
        endpoint
            .connect(args.server.parse()?, "localhost")?
            .await
            .context("Failed to connect to server")
    })?;
    println!("Connected to server at {}", args.server);

    // Send login message (blocking)
    rt.block_on(async {
        let stream = MessageStream::new(&connection);
        stream.send(&ClientMessage::Login(CLogin {})).await
    })?;

    // Channel for sending from the network I/O task to bevy
    let (to_bevy, from_server) = tokio::sync::mpsc::unbounded_channel();

    // Channel for sending from bevy to the network I/O task
    let (to_server, from_bevy) = tokio::sync::mpsc::unbounded_channel();

    // Spawn network I/O task (takes to_client, from_client from task's perspective)
    rt.spawn(network_io_task(connection, to_bevy, from_bevy));

    // Start Bevy app
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Game Client".to_string(),
                resolution: (1200.0, 800.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClientState::new())
        .insert_resource(BevyToServerChannel::new(to_server))
        .insert_resource(ServerToBevyChannel::new(from_server))
        .add_systems(Startup, setup_world)
        .add_systems(Update, (
            server_to_bevy_system,
            sync_players,
            update_player_positions,
        ).chain())
        .run();

    Ok(())
}
