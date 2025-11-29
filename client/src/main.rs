use anyhow::{Context, Result};
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, WindowPlugin, WindowPosition};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, time::Duration};

use client::{
    config::configure_client,
    net::network_io_task,
    resources::{CameraViewMode, ClientToServerChannel, PlayerMap, RoundTripTime, ServerToClientChannel},
    systems::{
        collision::client_hit_detection_system,
        effects::{apply_camera_shake_system, apply_cuboid_shake_system},
        input::{camera_view_toggle_system, cursor_toggle_system, input_system, shooting_input_system},
        movement::client_movement_system,
        network::{echo_system, process_server_events_system},
        sync::{
            sync_camera_to_player_system, sync_face_to_transform_system, sync_local_player_visibility_system,
            sync_position_to_transform_system, sync_projectiles_system,
        },
        ui::{setup_world_system, update_player_list_system, update_rtt_system},
        walls::spawn_walls_system,
    },
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

    let rt = Runtime::new()?;
    let connection = connect_to_server(&rt, args.server.as_str())?;
    send_login(&rt, &connection)?;

    // Channel for sending from the network I/O task to the client
    let (to_client, from_server) = tokio::sync::mpsc::unbounded_channel();
    // Channel for sending from the client to the network I/O task
    let (to_server, from_client) = tokio::sync::mpsc::unbounded_channel();

    let artificial_lag = (args.lag_ms > 0).then(|| Duration::from_millis(args.lag_ms));
    rt.spawn(network_io_task(connection, to_client, from_client, artificial_lag));

    let window_position = window_position_from_args(&args);

    // Start Bevy app
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(asset_plugin())
            .set(window_plugin(&args, window_position)),
    )
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
            // Client movement with wall collision
            client_movement_system,
            // Camera follows player
            sync_camera_to_player_system,
            // Update local player visibility based on view mode
            sync_local_player_visibility_system,
            // Sync Position to Transform
            sync_position_to_transform_system,
            // Sync Rotation to Transform
            sync_face_to_transform_system,
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

fn connect_to_server(rt: &Runtime, server_addr: &str) -> Result<quinn::Connection> {
    rt.block_on(async {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        let client_config = configure_client()?;
        endpoint.set_default_client_config(client_config);
        endpoint
            .connect(server_addr.parse()?, "localhost")?
            .await
            .context("failed to connect to server")
    })
}

fn send_login(rt: &Runtime, connection: &quinn::Connection) -> Result<()> {
    rt.block_on(async {
        let msg = ClientMessage::Login(CLogin {});
        let stream = MessageStream::new(connection);
        stream.send(&msg).await
    })
}

fn window_position_from_args(args: &Args) -> WindowPosition {
    match (args.window_x, args.window_y) {
        (Some(x), Some(y)) => WindowPosition::At(IVec2::new(x, y)),
        _ => WindowPosition::Automatic,
    }
}

fn asset_plugin() -> AssetPlugin {
    AssetPlugin {
        file_path: "assets".to_string(),
        ..default()
    }
}

fn window_plugin(args: &Args, position: WindowPosition) -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            title: "Game Client".to_string(),
            resolution: (args.window_width, args.window_height).into(),
            position,
            ..default()
        }),
        primary_cursor_options: Some(CursorOptions {
            visible: false,
            grab_mode: CursorGrabMode::Locked,
            hit_test: true,
        }),
        ..default()
    }
}
