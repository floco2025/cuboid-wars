use anyhow::{Context, Result};
use bevy::{
    pbr::DefaultOpaqueRendererMethod,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PresentMode, WindowPlugin, WindowPosition},
};
use clap::Parser;
use quinn::Endpoint;
use tokio::{runtime::Runtime, time::Duration};

use client::{
    actors::{ActorGhostMap, ActorMap},
    barriers::{OpenBarrierKinds, setup_barrier_assets},
    cameras::{CameraViewMode, TopDownCameraYaw, setup_cameras_system},
    characters::{character_sync_plugin, prediction_plugin},
    config::{AssetSet, ClientSettings, OpaqueRenderer},
    input::input_plugin,
    items::{ItemMap, setup_item_assets},
    map::{DebugColors, LevelFocusEnabled, map_plugin, setup_scene_lighting_system, sky_weather_plugin},
    materials::{GrassMaterialPlugin, generate_material_mipmaps_system},
    missiles::{LockOnTarget, MissileAssets, MissileMap},
    network::{
        ClientToServerChannel, LastSnapshotSeq, RoundTripTime, ServerToClientChannel, configure_client,
        network_io_task, network_plugin,
    },
    players::{LocalPlayerInfo, PlayerMap, camera_plugin},
    projectiles::{LastBounceSound, ProjectileAssets},
    schedule::configure_client_sets,
    ui::{
        ConsoleState, FpsMeasurement, GameMessageFeed, PendingBanner, QuestLog, SeenPlayerIds, hud_plugin,
        setup_ui_system,
    },
    vfx::{ExplosionAssets, ExplosionRadii, ExplosionVfxBudget, ParticleClouds, RainIntensity, presentation_plugin},
};
use common::{config::GameplayConfig, constants::TICK_HZ, network::MessageStream, protocol::*};

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

    // Master audio volume (1.0 = normal, 0 = mute). Muting one window is the
    // only way to judge spatial audio with two local clients on one machine.
    #[arg(long, default_value = "1.0")]
    volume: f32,
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    let args = Args::parse();
    let asset_set = AssetSet::load_default()?;
    let client_settings = ClientSettings::load_default()?;
    let gameplay_config = GameplayConfig::load_default()?;
    let barrier_kind_table = common::protocol::BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .context("failed to build BarrierKindTable from gameplay.json barrier_kinds")?;
    // Hard-fail if any kind lacks a color in assets.json — silent fallbacks
    // would make authoring errors mysterious.
    for id in barrier_kind_table.ids() {
        if asset_set.barrier_kind_color_hex(id).is_none() {
            anyhow::bail!(
                "barrier kind {id:?} has no color in assets.json `barrier_kind_colors`; add an entry or remove the id from gameplay.json"
            );
        }
    }
    let texture_mipmaps_enabled = client_settings.rendering.texture_mipmaps_enabled;

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

    // SIGINT (Ctrl-C in the launching terminal) is unreliable while the
    // macOS CFRunLoop / winit event loop is running — the signal can
    // queue without ever waking the loop. Spawn a tokio task that
    // services SIGINT and bypasses Bevy's shutdown entirely. Exit code
    // 130 follows the conventional "SIGINT" exit.
    rt.spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });

    let window_position = window_position_from_args(&args);

    // Start Bevy app
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(asset_plugin()).set(window_plugin(
        &args,
        window_position,
        client_settings.rendering.vsync,
    )));
    app.add_plugins(GrassMaterialPlugin);
    app.insert_resource(match client_settings.rendering.opaque_renderer {
        OpaqueRenderer::Auto => DefaultOpaqueRendererMethod::default(),
        OpaqueRenderer::Forward => DefaultOpaqueRendererMethod::forward(),
        OpaqueRenderer::Deferred => DefaultOpaqueRendererMethod::deferred(),
    });

    // Run client physics at the shared `TICK_HZ` tick (matches the server).
    // Render-time interpolation in the transform-sync systems hides the step
    // rate.
    app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)));

    app.insert_resource(bevy::audio::GlobalVolume::new(bevy::audio::Volume::Linear(
        args.volume.max(0.0),
    )));

    app.insert_resource(ClientToServerChannel::new(to_server))
        .insert_resource(ServerToClientChannel::new(from_server))
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ActorGhostMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(LocalPlayerInfo::default())
        .insert_resource(RoundTripTime::default())
        .insert_resource(FpsMeasurement::default())
        .insert_resource(LastSnapshotSeq::default())
        .insert_resource(OpenBarrierKinds::default())
        .insert_resource(CameraViewMode::default())
        .insert_resource(TopDownCameraYaw::default())
        .insert_resource(LevelFocusEnabled::default())
        .insert_resource(gameplay_config)
        .insert_resource(barrier_kind_table)
        .insert_resource(asset_set)
        .insert_resource(client_settings)
        .insert_resource(DebugColors::default())
        .insert_resource(LastBounceSound::default())
        .insert_resource(GameMessageFeed::default())
        .insert_resource(ConsoleState::default())
        .insert_resource(PendingBanner::default())
        .insert_resource(SeenPlayerIds::default())
        .insert_resource(QuestLog::default())
        .insert_resource(MissileMap::default())
        .insert_resource(LockOnTarget::default())
        .init_resource::<MissileAssets>()
        .init_resource::<client::ui::HudShapeAssets>()
        .init_resource::<ProjectileAssets>()
        .init_resource::<ParticleClouds>()
        .init_resource::<RainIntensity>()
        .init_resource::<ExplosionAssets>()
        .init_resource::<ExplosionRadii>()
        .init_resource::<ExplosionVfxBudget>()
        // Startup creates persistent scene infrastructure before any server
        // map data arrives.
        .add_systems(
            Startup,
            (
                setup_scene_lighting_system,
                setup_cameras_system,
                setup_ui_system,
                setup_item_assets,
                setup_barrier_assets,
            ),
        );

    // Cross-domain `Update` ordering (the `ClientSet` graph), then one
    // plugin per domain; each plugin owns its intra-set ordering.
    configure_client_sets(&mut app);
    app.add_plugins((
        input_plugin,
        network_plugin,
        prediction_plugin,
        character_sync_plugin,
        camera_plugin,
        presentation_plugin,
        map_plugin,
        hud_plugin,
        sky_weather_plugin,
    ));

    if texture_mipmaps_enabled {
        // Do not use bevy_mod_mipmap_generator::generate_mipmaps directly here.
        // It reacts to material events only once, while our materials often point
        // at image assets that are still loading. Our system retries until the
        // images exist, then calls the crate's mip generation function.
        app.add_systems(Update, generate_material_mipmaps_system);
    }

    app.run();

    // `app.run()` returns once `AppExit` is written (e.g., server
    // disconnect), but on macOS the tokio runtime drop + winit teardown
    // don't always fully release the process — it lingers with no main
    // loop running, immune to Ctrl-C. Force a clean process exit so the
    // user never has to force-quit.
    std::process::exit(0);
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

fn window_plugin(args: &Args, position: WindowPosition, vsync: bool) -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            title: "Cuboid Wars".to_string(),
            resolution: (args.window_width, args.window_height).into(),
            position,
            present_mode: if vsync {
                PresentMode::Fifo
            } else {
                PresentMode::AutoNoVsync
            },
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
