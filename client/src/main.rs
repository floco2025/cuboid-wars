use anyhow::{Context, Result};
use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, WindowPlugin, WindowPosition},
};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, time::Duration};

use client::{
    config::configure_client,
    net::network_io_task,
    resources::*,
    systems::{input::*, items::*, map::*, network::*, players::*, projectiles::*, sentries::*, skybox::*, ui::*},
};
use common::{net::MessageStream, protocol::*};

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

    // Invert mouse pitch (up/down)
    #[arg(short, long, default_value_t = false)]
    invert_pitch: bool,
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
    .insert_resource(SentryMap::default())
    .insert_resource(LocalPlayerInfo::default())
    .insert_resource(RoundTripTime::default())
    .insert_resource(FpsMeasurement::default())
    .insert_resource(LastUpdateSeq::default())
    .insert_resource(CameraViewMode::default())
    .insert_resource(RoofRenderingEnabled::default())
    .insert_resource(InputSettings {
        invert_pitch: args.invert_pitch,
    })
    .add_systems(
        Startup,
        (setup_world_system, setup_skybox_from_cross.after(setup_world_system)),
    )
    .add_systems(
        Update,
        (
            input_movement_system,
            input_shooting_system,
            input_cursor_toggle_system,
            input_camera_view_toggle_system,
            input_roof_toggle_system,
        ),
    )
    .add_systems(Update, (network_echo_system, network_server_message_system))
    .add_systems(
        Update,
        (
            players_movement_system,
            players_transform_sync_system,
            players_face_to_transform_system,
            players_billboard_system,
        ),
    )
    .add_systems(
        Update,
        (
            local_player_camera_shake_system,
            local_player_cuboid_shake_system,
            local_player_camera_sync_system,
            local_player_rearview_sync_system.after(input_movement_system), // Run after input sets camera rotation
            local_player_rearview_system,
            local_player_visibility_sync_system,
        ),
    )
    .add_systems(Update, (sentries_movement_system, sentries_transform_sync_system))
    .add_systems(Update, projectiles_movement_system)
    .add_systems(Update, items_animation_system)
    .add_systems(
        Update,
        (
            map_spawn_walls_system,
            map_toggle_wall_opacity_system,
            map_toggle_roof_visibility_system,
            map_make_wall_lights_emissive_system,
        ),
    )
    .add_systems(
        Update,
        (
            ui_toggle_crosshair_system,
            ui_player_list_system,
            ui_stunned_blink_system,
            ui_rtt_system,
            ui_fps_system,
        ),
    )
    .add_systems(
        Update,
        (
            skybox_convert_cross_to_cubemap_system.run_if(resource_exists::<SkyboxCrossImage>),
            skybox_update_camera_system.run_if(resource_exists::<SkyboxCubemap>),
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
