use anyhow::{Context, Result};
use bevy::{
    pbr::DefaultOpaqueRendererMethod,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, WindowPlugin, WindowPosition},
};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, time::Duration};

use client::{
    config::{AssetSet, OpaqueRenderer, RenderSettings, configure_client},
    materials::generate_material_mipmaps_system,
    net::network_io_task,
    resources::*,
    spawning::{ProjectileAssets, player_shadow_settings_system},
    systems::*,
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

    // Render walls and floors with random colors for debugging
    #[arg(long, default_value_t = false)]
    debug_colors: bool,
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    let args = Args::parse();
    let asset_set = AssetSet::load_default()?;
    let render_settings = RenderSettings::load_default()?;
    let debug_colors = args.debug_colors || render_settings.debug_map_colors;
    let texture_mipmaps_enabled = render_settings.texture_mipmaps_enabled;

    let player_name = args.name.clone().unwrap_or_else(|| {
        let full_name = whoami::realname().unwrap_or_default();
        let first_name = full_name.split_whitespace().next();
        first_name.unwrap_or_default().to_string()
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
    );
    app.insert_resource(match render_settings.opaque_renderer {
        OpaqueRenderer::Auto => DefaultOpaqueRendererMethod::default(),
        OpaqueRenderer::Forward => DefaultOpaqueRendererMethod::forward(),
        OpaqueRenderer::Deferred => DefaultOpaqueRendererMethod::deferred(),
    });

    app.insert_resource(ClientToServerChannel::new(to_server))
        .insert_resource(ServerToClientChannel::new(from_server))
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(LocalPlayerInfo::default())
        .insert_resource(RoundTripTime::default())
        .insert_resource(FpsMeasurement::default())
        .insert_resource(LastUpdateSeq::default())
        .insert_resource(CameraViewMode::default())
        .insert_resource(TopDownCameraYaw::default())
        .insert_resource(LevelFocusEnabled::default())
        .insert_resource(InputSettings {
            invert_pitch: args.invert_pitch,
        })
        .insert_resource(asset_set)
        .insert_resource(render_settings)
        .insert_resource(DebugColors(debug_colors))
        .insert_resource(LastBounceSoundTime::default())
        .init_resource::<ProjectileAssets>()
        .add_systems(
            Startup,
            (
                setup_world_geometry_system,
                setup_cameras_system,
                setup_ui_system,
                setup_skybox_from_cross.after(setup_world_geometry_system),
            ),
        )
        .add_systems(
            Update,
            (
                input_movement_system.after(input_camera_view_toggle_system),
                input_shooting_system.after(input_movement_system),
                input_cursor_toggle_system,
                input_camera_view_toggle_system,
                input_level_focus_toggle_system,
                input_fullscreen_toggle_system,
            ),
        )
        .add_systems(Update, (network_echo_system, network_server_message_system))
        .add_systems(
            Update,
            (
                characters_movement_system,
                players_transform_sync_system.after(characters_movement_system),
                players_face_to_transform_system,
                players_billboard_system,
                actors_transform_sync_system.after(characters_movement_system),
            ),
        )
        .add_systems(
            Update,
            (
                local_player_camera_shake_system,
                local_player_cuboid_shake_system,
                local_player_camera_sync_system
                    .after(input_movement_system)
                    .after(local_player_camera_shake_system),
                local_player_rearview_sync_system.after(local_player_camera_sync_system),
                local_player_rearview_system.after(local_player_rearview_sync_system),
                local_player_visibility_sync_system.after(input_camera_view_toggle_system),
            ),
        )
        .add_systems(Update, projectiles_movement_system)
        .add_systems(Update, player_shadow_settings_system)
        .add_systems(Update, items_animation_system)
        .add_systems(
            Update,
            (
                map_spawn_geometry_system,
                map_level_focus_visibility_system,
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
        );

    if texture_mipmaps_enabled {
        // Do not use bevy_mod_mipmap_generator::generate_mipmaps directly here.
        // It reacts to material events only once, while our materials often point
        // at image assets that are still loading. Our system retries until the
        // images exist, then calls the crate's mip generation function.
        app.add_systems(Update, generate_material_mipmaps_system);
    }

    app.run();

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
