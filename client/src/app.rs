use anyhow::{Context, Result};
use bevy::{
    pbr::DefaultOpaqueRendererMethod,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PresentMode, WindowPlugin, WindowPosition},
};

use crate::{
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
    network::{ClientToServerChannel, LastSnapshotSeq, RoundTripTime, ServerToClientChannel, network_plugin},
    players::{LocalPlayerInfo, PlayerMap, camera_plugin},
    projectiles::{LastBounceSound, ProjectileAssets},
    schedule::configure_client_sets,
    ui::{
        ConsoleState, FpsMeasurement, GameMessageFeed, HudShapeAssets, PendingBanner, QuestLog, SeenPlayerIds,
        hud_plugin, setup_ui_system,
    },
    vfx::{ExplosionAssets, ExplosionRadii, ExplosionVfxBudget, ParticleClouds, RainIntensity, presentation_plugin},
};
use common::{config::GameplayConfig, constants::TICK_HZ, protocol::BarrierKindTable};

pub struct ClientAppOptions {
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: u32,
    pub window_height: u32,
    pub volume: f32,
}

pub fn build_client_app(
    options: ClientAppOptions,
    to_server: ClientToServerChannel,
    from_server: ServerToClientChannel,
) -> Result<App> {
    let asset_set = AssetSet::load_default()?;
    let client_settings = ClientSettings::load_default()?;
    let gameplay_config = GameplayConfig::load_default()?;
    let barrier_kind_table = BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .context("failed to build BarrierKindTable from gameplay.json barrier_kinds")?;
    asset_set.validate_gameplay_bindings(&gameplay_config, &barrier_kind_table)?;

    let texture_mipmaps_enabled = client_settings.rendering.texture_mipmaps_enabled;
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(asset_plugin())
            .set(window_plugin(&options, client_settings.rendering.vsync)),
    );
    app.add_plugins(GrassMaterialPlugin);
    app.insert_resource(match client_settings.rendering.opaque_renderer {
        OpaqueRenderer::Auto => DefaultOpaqueRendererMethod::default(),
        OpaqueRenderer::Forward => DefaultOpaqueRendererMethod::forward(),
        OpaqueRenderer::Deferred => DefaultOpaqueRendererMethod::deferred(),
    });

    app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)));
    app.insert_resource(bevy::audio::GlobalVolume::new(bevy::audio::Volume::Linear(
        options.volume.max(0.0),
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
        .init_resource::<HudShapeAssets>()
        .init_resource::<ProjectileAssets>()
        .init_resource::<ParticleClouds>()
        .init_resource::<RainIntensity>()
        .init_resource::<ExplosionAssets>()
        .init_resource::<ExplosionRadii>()
        .init_resource::<ExplosionVfxBudget>()
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

fn window_plugin(options: &ClientAppOptions, vsync: bool) -> WindowPlugin {
    let position = match (options.window_x, options.window_y) {
        (Some(x), Some(y)) => WindowPosition::At(IVec2::new(x, y)),
        _ => WindowPosition::Automatic,
    };
    WindowPlugin {
        primary_window: Some(Window {
            title: "Cuboid Wars".to_string(),
            resolution: (options.window_width, options.window_height).into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn options(window_x: Option<i32>, window_y: Option<i32>) -> ClientAppOptions {
        ClientAppOptions {
            window_x,
            window_y,
            window_width: 1200,
            window_height: 800,
            volume: 1.0,
        }
    }

    #[test]
    fn window_position_requires_both_coordinates() {
        let plugin = window_plugin(&options(Some(10), None), true);
        let window = plugin.primary_window.expect("primary window should be configured");
        assert_eq!(window.position, WindowPosition::Automatic);
    }

    #[test]
    fn window_position_uses_both_coordinates() {
        let plugin = window_plugin(&options(Some(10), Some(20)), true);
        let window = plugin.primary_window.expect("primary window should be configured");
        assert_eq!(window.position, WindowPosition::At(IVec2::new(10, 20)));
    }
}
