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
    actors::{ActorMap, actors_transform_sync_system},
    barriers::{
        OpenBarrierKinds, barriers_pulsate_system, barriers_spawn_system, barriers_visibility_system,
        pressure_plates_spawn_system, setup_barrier_assets,
    },
    cameras::{CameraViewMode, TopDownCameraYaw, setup_cameras_system},
    characters::{capture_previous_tick_position_system, characters_movement_system, characters_visual_turn_system},
    config::{AssetSet, ClientSettings, OpaqueRenderer, configure_client},
    input::{
        commit_player_input_system, input_camera_view_toggle_system, input_cursor_toggle_system,
        input_debug_colors_cycle_system, input_fullscreen_toggle_system, input_level_focus_toggle_system,
        input_movement_system, input_shooting_system,
    },
    items::{ItemMap, items_animation_system, setup_item_assets, y_spin_system},
    map::{
        DebugColors, LevelFocusEnabled, grass_spawn_system, map_level_focus_visibility_system,
        map_spawn_geometry_system, map_wall_light_emissive_system, setup_scene_lighting_system,
    },
    materials::{GrassMaterialPlugin, generate_material_mipmaps_system},
    network::{
        ClientToServerChannel, LastSnapshotSeq, RoundTripTime, ServerToClientChannel, network_io_task,
        network_ping_system, network_process_server_messages_system,
    },
    players::{
        LocalPlayerInfo, PlayerMap, death_overlay_visibility_system, local_player_camera_shake_system,
        local_player_camera_sync_system, local_player_cuboid_shake_system, local_player_rearview_sync_system,
        local_player_rearview_viewport_system, local_player_visibility_sync_system, players_transform_sync_system,
    },
    projectiles::{
        LastBounceSoundTime, ProjectileAssets, projectiles_movement_system, projectiles_transform_sync_system,
    },
    skybox::{setup_skybox_from_cross_system, skybox_convert_cross_to_cubemap_system, skybox_update_camera_system},
    ui::{
        FpsMeasurement, GameMessageFeed, QuestLog, SeenPlayerIds,
        floating_labels::{
            floating_health_bar_fill_system, floating_labels_billboard_system, player_name_label_render_system,
        },
        render_pending_messages_system, setup_ui_system, tick_hud_banner_system, ui_crosshair_visibility_system,
        ui_fps_system, ui_health_bar_fill_system, ui_player_list_rebuild_system, ui_quest_panel_rebuild_system,
        ui_rtt_system, ui_stunned_blink_system, update_message_feed_system,
    },
    vfx::explosion_effects_system,
};
use common::{config::GameplayConfig, constants::TICK_HZ, net::MessageStream, protocol::*};

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
    app.add_plugins(
        DefaultPlugins
            .set(asset_plugin())
            .set(window_plugin(&args, window_position)),
    );
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

    app.insert_resource(ClientToServerChannel::new(to_server))
        .insert_resource(ServerToClientChannel::new(from_server))
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
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
        .insert_resource(LastBounceSoundTime::default())
        .insert_resource(GameMessageFeed::default())
        .insert_resource(SeenPlayerIds::default())
        .insert_resource(QuestLog::default())
        .init_resource::<ProjectileAssets>()
        // Startup creates persistent scene infrastructure before any server
        // map data arrives.
        .add_systems(
            Startup,
            (
                setup_scene_lighting_system,
                setup_cameras_system,
                setup_ui_system,
                setup_skybox_from_cross_system.after(setup_scene_lighting_system),
                setup_item_assets,
                setup_barrier_assets,
            ),
        )
        // Input writes local intent and view/debug state.
        .add_systems(
            Update,
            (
                input_movement_system.after(input_camera_view_toggle_system),
                input_shooting_system.after(input_movement_system),
                // Run before shooting (which is after movement) so a click that
                // re-locks the cursor also fires that same frame, instead of
                // depending on nondeterministic system order.
                input_cursor_toggle_system.before(input_movement_system),
                input_camera_view_toggle_system,
                input_level_focus_toggle_system,
                input_fullscreen_toggle_system,
                input_debug_colors_cycle_system,
            ),
        )
        // Network consumes server messages and sends periodic ping requests.
        .add_systems(Update, (network_ping_system, network_process_server_messages_system))
        // Character prediction runs at the shared `TICK_HZ` tick. The
        // commit system sends the player's input to the server before
        // physics consumes it (so what physics simulates is what was sent).
        // The capture system stamps `PreviousTickPosition` before movement
        // so the render-rate transform sync can interpolate.
        .add_systems(
            FixedUpdate,
            (
                commit_player_input_system,
                capture_previous_tick_position_system,
                characters_movement_system,
                // Projectiles step at the same fixed tick as the server so
                // the step-size-dependent integration doesn't diverge from
                // the authoritative trajectories.
                projectiles_movement_system,
            )
                .chain(),
        )
        // Transform sync runs every render frame and lerps between the last
        // two ticks' positions so motion looks smooth above 30 Hz.
        .add_systems(
            Update,
            (
                players_transform_sync_system,
                actors_transform_sync_system,
                characters_visual_turn_system
                    .after(players_transform_sync_system)
                    .after(actors_transform_sync_system),
                floating_labels_billboard_system,
                player_name_label_render_system,
                floating_health_bar_fill_system,
            ),
        )
        // Cameras follow the local player after input/prediction has had a
        // chance to update the player state.
        .add_systems(
            Update,
            (
                local_player_camera_shake_system,
                local_player_cuboid_shake_system,
                local_player_camera_sync_system
                    .after(input_movement_system)
                    .after(local_player_camera_shake_system),
                local_player_rearview_sync_system.after(local_player_camera_sync_system),
                local_player_rearview_viewport_system.after(local_player_rearview_sync_system),
                local_player_visibility_sync_system.after(input_camera_view_toggle_system),
            ),
        )
        // Client-side presentation systems that animate non-character entities.
        .add_systems(
            Update,
            (
                projectiles_transform_sync_system,
                explosion_effects_system,
                items_animation_system,
                y_spin_system,
            ),
        )
        // Map rendering systems are mostly one-shot or visibility/material
        // maintenance driven by loaded assets and level focus.
        .add_systems(
            Update,
            (
                map_spawn_geometry_system,
                grass_spawn_system,
                map_level_focus_visibility_system,
                map_wall_light_emissive_system,
                barriers_spawn_system,
                barriers_pulsate_system,
                // After `map_level_focus_visibility_system` so the open-kind
                // override wins the per-frame race for barrier visibility.
                barriers_visibility_system.after(map_level_focus_visibility_system),
                pressure_plates_spawn_system,
            ),
        )
        // HUD and screen-space UI.
        .add_systems(
            Update,
            (
                ui_crosshair_visibility_system,
                ui_player_list_rebuild_system,
                ui_health_bar_fill_system.after(ui_player_list_rebuild_system),
                ui_quest_panel_rebuild_system,
                ui_stunned_blink_system,
                ui_rtt_system,
                ui_fps_system,
                death_overlay_visibility_system,
                render_pending_messages_system,
                update_message_feed_system,
                tick_hud_banner_system,
            ),
        )
        // Skybox asset conversion and camera following.
        .add_systems(
            Update,
            (
                skybox_convert_cross_to_cubemap_system.run_if(resource_exists::<client::skybox::SkyboxCrossImage>),
                skybox_update_camera_system.run_if(resource_exists::<client::skybox::SkyboxCubemap>),
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
