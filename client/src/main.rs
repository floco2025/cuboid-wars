use anyhow::{Context, Result};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, WindowPlugin, WindowPosition};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, time::Duration};

use client::{
    config::configure_client,
    net::network_io_task,
    resources::{
        CameraViewMode, ClientToServerChannel, FpsMeasurement, GhostMap, ItemMap, LastUpdateSeq, PlayerMap,
        RoofRenderingEnabled, RoundTripTime, ServerToClientChannel,
    },
    systems::{
        items::animate_items_system,
        effects::{apply_camera_shake_system, apply_cuboid_shake_system},
        ghosts::ghost_movement_system,
        input::{
            camera_view_toggle_system, cursor_toggle_system, input_system, roof_toggle_system, shooting_input_system,
        },
        network::{echo_system, process_server_events_system},
        players::player_movement_system,
        projectiles::{client_hit_detection_system, sync_projectiles_system},
        sync::{
            billboard_player_id_text_system, sync_camera_to_player_system, sync_face_to_transform_system,
            sync_local_player_visibility_system, sync_position_to_transform_system,
        },
        ui::{
            setup_world_system, toggle_crosshair_system, update_fps_system, update_player_list_system,
            update_rtt_system,
        },
        walls::{spawn_walls_system, toggle_roof_visibility_system, toggle_wall_opacity_system},
    },
};
use common::net::MessageStream;
use common::protocol::*;

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(author, version, about = "Cuboid Wars", long_about = None)]
struct Args {
    // Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,

    // Player name to display
    #[arg(short, long)]
    name: Option<String>,

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

    let player_name = args.name.clone().unwrap_or_else(|| {
        let full_name = whoami::realname();
        let first_name = full_name.split_whitespace().next();
        first_name.unwrap_or("").to_string()
    });

    let rt = Runtime::new()?;
    let connection = connect_to_server(&rt, args.server.as_str())?;
    send_login(&rt, &connection, &player_name)?;

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
    .insert_resource(ItemMap::default())
    .insert_resource(GhostMap::default())
    .insert_resource(RoundTripTime::default())
    .insert_resource(FpsMeasurement::default())
    .insert_resource(LastUpdateSeq::default())
    .insert_resource(CameraViewMode::default())
    .insert_resource(RoofRenderingEnabled::default())
    .add_systems(Startup, setup_world_system)
    .add_systems(
        Update,
        (
            cursor_toggle_system,
            camera_view_toggle_system,
            roof_toggle_system,
            input_system,
            shooting_input_system,
        ),
    )
    .add_systems(Update, process_server_events_system)
    .add_systems(
        Update,
        (
            echo_system,
            spawn_walls_system,
            player_movement_system,
            ghost_movement_system,
            sync_projectiles_system,
        ),
    )
    .add_systems(
        Update,
        (
            client_hit_detection_system,
            sync_camera_to_player_system,
            sync_position_to_transform_system,
            sync_face_to_transform_system,
        ),
    )
    .add_systems(
        Update,
        (
            sync_local_player_visibility_system,
            billboard_player_id_text_system,
            animate_items_system,
            apply_camera_shake_system,
        ),
    )
    .add_systems(
        Update,
        (
            billboard_player_id_text_system,
            animate_items_system,
            apply_camera_shake_system,
            apply_cuboid_shake_system,
            toggle_wall_opacity_system,
        ),
    )
    .add_systems(
        Update,
        (
            toggle_roof_visibility_system,
            toggle_crosshair_system,
            update_player_list_system,
            update_rtt_system,
            update_fps_system,
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

fn send_login(rt: &Runtime, connection: &quinn::Connection, name: &str) -> Result<()> {
    rt.block_on(async {
        let msg = ClientMessage::Login(CLogin { name: name.to_string() });
        let stream = MessageStream::new(connection);
        stream.send(&msg).await
    })
}

const fn window_position_from_args(args: &Args) -> WindowPosition {
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
            title: "Cuboid Wars".to_string(),
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
