use anyhow::{Context, Result};
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;

use client::{
    config::configure_client,
    net::network_io_task,
    resources::{CameraViewMode, ClientToServerChannel, PlayerMap, RoundTripTime, ServerToClientChannel},
    systems::{
        collision::client_hit_detection_system,
        effects::{apply_camera_shake_system, apply_cuboid_shake_system},
        input::{camera_view_toggle_system, cursor_toggle_system, input_system, shooting_input_system},
        network::{echo_system, process_server_events_system},
        sync::{
            sync_camera_to_player_system, sync_local_player_visibility_system,
            sync_position_to_transform_system, sync_rotation_to_transform_system,
            sync_projectiles_system,
        },
        ui::{setup_world_system, update_player_list_system, update_rtt_system},
        walls::spawn_walls_system,
    },
};
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use common::{net::MessageStream, systems::movement_system};

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(author, version, about = "Game client", long_about = None)]
struct Args {
    // Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,

    // Simulated network lag in milliseconds
    #[arg(long, default_value = "0")]
    lag_ms: u64,

    // Window X position
    #[arg(long)]
    window_x: Option<i32>,

    // Window Y position
    #[arg(long)]
    window_y: Option<i32>,

    // Window width
    #[arg(long, default_value = "1200")]
    window_width: u32,

    // Window height
    #[arg(long, default_value = "800")]
    window_height: u32,
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    let args = Args::parse();

    // Create tokio runtime for network I/O
    let rt = tokio::runtime::Runtime::new()?;

    // Connect to server (blocking)
    let connection = rt.block_on(async {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        let client_config = configure_client()?;
        endpoint.set_default_client_config(client_config);
        endpoint
            .connect(args.server.parse()?, "localhost")?
            .await
            .context("failed to connect to server")
    })?;
    // info! doesn't work because Bevy isn't inialized yet
    //info!("connected to server at {}", args.server);

    // Send login message (blocking)
    rt.block_on(async {
        let msg = ClientMessage::Login(CLogin {});
        // Trace doesn't work because Bevy isn't inialized yet
        //trace!("sending to server: {:?}", msg);
        let stream = MessageStream::new(&connection);
        stream.send(&msg).await
    })?;

    // Channel for sending from the network I/O task to the client
    let (to_client, from_server) = tokio::sync::mpsc::unbounded_channel();

    // Channel for sending from the client to the network I/O task
    let (to_server, from_client) = tokio::sync::mpsc::unbounded_channel();

    // Spawn network I/O task
    let lag_ms = args.lag_ms;
    rt.spawn(network_io_task(connection, to_client, from_client, lag_ms));

    // Configure window position
    let window_position = if let (Some(x), Some(y)) = (args.window_x, args.window_y) {
        bevy::window::WindowPosition::At(IVec2::new(x, y))
    } else {
        bevy::window::WindowPosition::Automatic
    };

    // Start Bevy app
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Game Client".to_string(),
            resolution: (args.window_width, args.window_height).into(),
            position: window_position,
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
    .insert_resource(PlayerMap::default())
    .insert_resource(RoundTripTime::default())
    .insert_resource(CameraViewMode::default())
    .add_systems(Startup, setup_world_system)
    .add_systems(
        Update,
        (
            // Toggle cursor lock with Escape
            cursor_toggle_system,
            // Toggle camera view with V
            camera_view_toggle_system,
            // Handle WASD input and mouse
            input_system,
            // Handle shooting input
            shooting_input_system,
            // Sync projectile physics to transforms
            sync_projectiles_system,
            // Client-side hit detection for visual despawning
            client_hit_detection_system,
            // Process server messages
            process_server_events_system,
            // Spawn walls when WallConfig is received
            spawn_walls_system,
            // Shared movement logic
            movement_system,
            // Camera follows player
            sync_camera_to_player_system,
            // Update local player visibility based on view mode
            sync_local_player_visibility_system,
            // Sync Position to Transform
            sync_position_to_transform_system,
            // Sync Rotation to Transform
            sync_rotation_to_transform_system,
            // Apply camera shake effects
            apply_camera_shake_system,
            // Apply cuboid shake effects
            apply_cuboid_shake_system,
            // Update player list UI
            update_player_list_system,
            // Update RTT display
            update_rtt_system,
            // Send echo requests and process responses
            echo_system,
        ),
    )
    .run();

    Ok(())
}
