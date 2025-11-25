use anyhow::{Context, Result};
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;

use client::{
    config::configure_client,
    net::network_io_task,
    resources::{ClientToServerChannel, ServerToClientChannel},
    systems::{cursor_toggle_system, input_system, process_server_events_system, setup_world_system, sync_camera_to_player_system, sync_position_to_transform_system, sync_rotation_to_transform_system},
};
use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(author, version, about = "Game client", long_about = None)]
struct Args {
    // Server address to connect to
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

    // Channel for sending from the network I/O task to the client
    let (to_client, from_server) = tokio::sync::mpsc::unbounded_channel();

    // Channel for sending from the client to the network I/O task
    let (to_server, from_client) = tokio::sync::mpsc::unbounded_channel();

    // Spawn network I/O task
    rt.spawn(network_io_task(connection, to_client, from_client));

    // Start Bevy app
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Game Client".to_string(),
                resolution: (1200, 800).into(),
                ..default()
            }),
            primary_cursor_options: Some(bevy::window::CursorOptions {
                visible: false,
                grab_mode: bevy::window::CursorGrabMode::Locked,
                hit_test: true,
            }),
            ..default()
        }))
        .insert_resource(ClientToServerChannel::new(to_server))
        .insert_resource(ServerToClientChannel::new(from_server))
        .add_systems(Startup, setup_world_system)
        .add_systems(
            Update,
            (
                cursor_toggle_system,                // Toggle cursor lock with Escape
                input_system,                        // Handle WASD input and mouse
                process_server_events_system,       // Process server messages
                common::systems::movement_system,     // Shared movement logic
                sync_camera_to_player_system,         // Camera follows player
                sync_position_to_transform_system,    // Sync Position to Transform
                sync_rotation_to_transform_system,    // Sync Rotation to Transform
            ),
        )
        .run();

    Ok(())
}
