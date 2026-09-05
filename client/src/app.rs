use anyhow::Result;
use bevy::{
    pbr::DefaultOpaqueRendererMethod,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, MonitorSelection, PresentMode, WindowMode, WindowPlugin, WindowPosition},
};

use crate::{
    actors::{ActorGhostMap, ActorMap},
    barriers::{LockedPlatePurposes, PlateState},
    cameras::{CameraViewMode, TopDownCameraYaw, clamp_msaa_to_device_system, setup_cameras_system},
    characters::{character_sync_plugin, prediction_plugin},
    config::{AssetSet, ClientSettings, LocalSettings, OpaqueRenderer},
    input::{WeaponMode, input_plugin},
    items::{ItemMap, setup_item_assets},
    map::{DebugColors, LevelFocusEnabled, map_plugin, setup_scene_lighting_system, sky_weather_plugin},
    materials::{GrassMaterialPlugin, generate_material_mipmaps_system},
    missiles::{LockOnTarget, MissileAssets, MissileMap},
    network::{
        ClientToServerChannel, LastPlayerMovesSeq, LastSnapshotSeq, RoundTripTime, ServerToClientChannel,
        install_bootstrap, network_plugin,
    },
    players::{LocalPlayerInfo, PlayerMap, camera_plugin},
    portals::{PortalAssets, PortalMap, portal_render_plugin},
    projectiles::LastBounceSound,
    schedule::configure_client_sets,
    ui::{ConsoleState, FpsMeasurement, HudShapeAssets, MessageFeed, hud_plugin, setup_ui_system},
    vfx::{ExplosionAssets, ExplosionVfxBudget, ParticleClouds, RainIntensity, presentation_plugin},
};
use common::{constants::TICK_HZ, physics::PortalSet, protocol::SInit};

pub struct ClientAppOptions {
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: u32,
    pub window_height: u32,
    pub volume: Option<f32>,
}

pub fn build_client_app(
    options: ClientAppOptions,
    to_server: ClientToServerChannel,
    from_server: ServerToClientChannel,
    bootstrap: SInit,
) -> Result<App> {
    let asset_set = AssetSet::load_default()?;
    let mut client_settings = ClientSettings::load_default()?;
    let local_settings = LocalSettings::load();
    if let Some(local) = &local_settings {
        local.apply_to(&mut client_settings);
        if let Err(error) = client_settings.validate() {
            // Pre-logger, so straight to stderr like the loader's warnings.
            eprintln!("warning: ignoring client_local.json: {error}");
            client_settings = ClientSettings::load_default()?;
        }
    }
    let start_fullscreen = local_settings.as_ref().is_some_and(|local| local.fullscreen);
    let mipmaps = client_settings.rendering.mipmaps;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(asset_plugin()).set(window_plugin(
        &options,
        client_settings.rendering.vsync,
        start_fullscreen,
    )));
    app.add_plugins(GrassMaterialPlugin);
    app.insert_resource(match client_settings.rendering.opaque_renderer {
        OpaqueRenderer::Auto => DefaultOpaqueRendererMethod::default(),
        OpaqueRenderer::Forward => DefaultOpaqueRendererMethod::forward(),
        OpaqueRenderer::Deferred => DefaultOpaqueRendererMethod::deferred(),
    });

    app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)));
    // Master volume precedence: CLI flag, then the saved settings, then 1.0.
    let volume = options
        .volume
        .or(local_settings.as_ref().map(|local| local.master_volume))
        .unwrap_or(1.0);
    app.insert_resource(bevy::audio::GlobalVolume::new(bevy::audio::Volume::Linear(
        volume.max(0.0),
    )));

    app.insert_resource(to_server)
        .insert_resource(from_server)
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ActorGhostMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(LocalPlayerInfo::default())
        .insert_resource(RoundTripTime::default())
        .insert_resource(FpsMeasurement::default())
        .insert_resource(LastSnapshotSeq::default())
        .insert_resource(LastPlayerMovesSeq::default())
        .insert_resource(PlateState::default())
        .insert_resource(LockedPlatePurposes::default())
        .insert_resource(CameraViewMode::default())
        .insert_resource(TopDownCameraYaw::default())
        .insert_resource(LevelFocusEnabled::default())
        .insert_resource(asset_set.clone())
        .insert_resource(client_settings)
        .insert_resource(DebugColors::default())
        .insert_resource(LastBounceSound::default())
        .insert_resource(MessageFeed::default())
        .insert_resource(ConsoleState::default())
        .insert_resource(MissileMap::default())
        .insert_resource(LockOnTarget::default())
        .insert_resource(PortalMap::default())
        .insert_resource(PortalSet::default())
        .insert_resource(WeaponMode::default())
        .init_resource::<PortalAssets>()
        .init_resource::<MissileAssets>()
        .init_resource::<HudShapeAssets>()
        .init_resource::<ParticleClouds>()
        .init_resource::<RainIntensity>()
        .init_resource::<ExplosionAssets>()
        .init_resource::<ExplosionVfxBudget>()
        .add_systems(
            Startup,
            (
                setup_scene_lighting_system,
                clamp_msaa_to_device_system.before(setup_cameras_system),
                setup_cameras_system,
                setup_ui_system,
                setup_item_assets,
            ),
        );

    app.add_systems(
        FixedUpdate,
        crate::portals::portal_transit_system
            .after(crate::characters::characters_movement_system)
            .before(crate::characters::knockback_decay_system),
    );

    configure_client_sets(&mut app);
    app.add_plugins((
        input_plugin,
        network_plugin,
        prediction_plugin,
        character_sync_plugin,
        camera_plugin,
        portal_render_plugin,
        presentation_plugin,
        map_plugin,
        hud_plugin,
        sky_weather_plugin,
    ));

    install_bootstrap(&mut app, bootstrap, &asset_set)?;

    if mipmaps {
        // Materials often reference images that are still loading when their
        // material event arrives, so this retrying system owns generation.
        app.add_systems(Update, generate_material_mipmaps_system);
    }

    Ok(app)
}

fn asset_plugin() -> AssetPlugin {
    AssetPlugin {
        file_path: "assets".to_string(),
        ..default()
    }
}

fn window_plugin(options: &ClientAppOptions, vsync: bool, fullscreen: bool) -> WindowPlugin {
    let position = match (options.window_x, options.window_y) {
        (Some(x), Some(y)) => WindowPosition::At(IVec2::new(x, y)),
        _ => WindowPosition::Automatic,
    };
    WindowPlugin {
        primary_window: Some(Window {
            title: "Cuboid Wars".to_string(),
            resolution: (options.window_width, options.window_height).into(),
            position,
            mode: if fullscreen {
                WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
            } else {
                WindowMode::Windowed
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn options(window_x: Option<i32>, window_y: Option<i32>) -> ClientAppOptions {
        ClientAppOptions {
            window_x,
            window_y,
            window_width: 1200,
            window_height: 800,
            volume: None,
        }
    }

    #[test]
    fn window_position_requires_both_coordinates() {
        let plugin = window_plugin(&options(Some(10), None), true, false);
        let window = plugin.primary_window.expect("primary window should be configured");
        assert_eq!(window.position, WindowPosition::Automatic);
    }

    #[test]
    fn window_position_uses_both_coordinates() {
        let plugin = window_plugin(&options(Some(10), Some(20)), true, false);
        let window = plugin.primary_window.expect("primary window should be configured");
        assert_eq!(window.position, WindowPosition::At(IVec2::new(10, 20)));
    }

    #[test]
    fn initial_fullscreen_selects_primary_monitor() {
        let plugin = window_plugin(&options(None, None), true, true);
        let window = plugin.primary_window.expect("primary window should be configured");
        assert_eq!(window.mode, WindowMode::BorderlessFullscreen(MonitorSelection::Primary));
    }
}
