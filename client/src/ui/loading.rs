use bevy::{
    asset::UntypedAssetLoadFailedEvent,
    input::{
        InputSystems,
        keyboard::KeyboardInput,
        mouse::{MouseMotion, MouseWheel},
    },
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp,
        render_resource::{CachedPipelineState, PipelineCache},
    },
    world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceSpawner},
};

use crate::{
    barriers::PressurePlateMarker,
    characters::CharacterModel,
    config::ClientSettings,
    map::{GrassChunkBuild, GrassChunks},
    materials::{MaterialMipmapState, MaterialTextures, TreeMaterial},
    players::LocalPlayerMarker,
    schedule::ClientSet,
};

const READY_FRAMES: u8 = 5;

#[derive(Resource, Default)]
struct StartupLoading {
    ready_frames: u8,
    failed: bool,
}

#[derive(Resource, Default)]
struct LoadingPipelines {
    ready: bool,
    failed: bool,
}

#[derive(Component)]
struct LoadingScreen;

#[derive(Component)]
struct LoadingText;

pub(crate) fn loading_screen_plugin(app: &mut App) {
    app.init_resource::<StartupLoading>()
        .init_resource::<LoadingPipelines>()
        .add_systems(Startup, spawn_loading_screen)
        .add_systems(
            PreUpdate,
            suppress_loading_input
                .after(InputSystems)
                .before(ClientSet::Console)
                .before(ClientSet::Input)
                .run_if(resource_exists::<StartupLoading>),
        )
        .add_systems(Last, finish_loading.run_if(resource_exists::<StartupLoading>));
    app.sub_app_mut(RenderApp)
        .add_systems(ExtractSchedule, report_loading_pipelines);
}

fn spawn_loading_screen(mut commands: Commands, settings: Res<ClientSettings>) {
    // Cover the scene image and HUD in their own UI pass: the opaque cover
    // shares their pipeline, so the HUD cannot render before the cover is
    // ready. A separate camera can compile its output pipeline a frame later
    // and briefly expose quest banners. Scene cameras keep warming underneath.
    commands
        .spawn((
            LoadingScreen,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(100),
            BackgroundColor(Color::BLACK),
        ))
        .with_child((
            LoadingText,
            Text::new("Loading..."),
            TextFont {
                font_size: FontSize::Px(settings.hud.font_sizes.settings_menu),
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
        ));
}

fn suppress_loading_input(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut keys: ResMut<Messages<KeyboardInput>>,
    mut motion: ResMut<Messages<MouseMotion>>,
    mut wheel: ResMut<Messages<MouseWheel>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    keyboard.reset_all();
    mouse.reset_all();
    keys.clear();
    motion.clear();
    wheel.clear();
}

fn finish_loading(
    mut commands: Commands,
    mut loading: ResMut<StartupLoading>,
    pipelines: Res<LoadingPipelines>,
    textures: Res<MaterialTextures>,
    mipmaps: Res<MaterialMipmapState>,
    grass: Res<GrassChunks>,
    grass_builds: Query<(), With<GrassChunkBuild>>,
    pending_models: Query<
        (),
        (
            Or<(With<CharacterModel>, With<PressurePlateMarker>)>,
            Without<WorldAssetRoot>,
        ),
    >,
    scenes: Query<(&WorldAssetRoot, Option<&WorldInstance>)>,
    spawner: Res<WorldInstanceSpawner>,
    server: Res<AssetServer>,
    trees: Res<Assets<TreeMaterial>>,
    players: Query<(), With<LocalPlayerMarker>>,
    mut failures: MessageReader<UntypedAssetLoadFailedEvent>,
    mut text: Query<&mut Text, With<LoadingText>>,
    screens: Query<Entity, With<LoadingScreen>>,
) {
    if loading.failed {
        return;
    }
    loading.failed = failures.read().count() > 0 || pipelines.failed;
    if loading.failed {
        if let Ok(mut text) = text.single_mut() {
            text.0 = "Unable to load the game.\nPress Esc to quit.".into();
        }
        return;
    }
    let ready = !players.is_empty()
        && !textures.is_loading()
        && !mipmaps.is_loading()
        && !grass.waiting_for_chunks
        && grass_builds.is_empty()
        && pending_models.is_empty()
        && scenes.iter().all(|(root, instance)| {
            server.is_loaded_with_dependencies(&root.0)
                && instance.is_some_and(|instance| spawner.instance_is_ready(**instance))
        })
        && trees.iter().all(|(_, tree)| server.are_dependencies_loaded(&tree.base))
        && pipelines.ready;
    // Asset extraction, material preparation, and pipeline queuing span frames.
    // Confirm readiness across rendered frames instead of using a timed delay.
    loading.ready_frames = if ready { loading.ready_frames + 1 } else { 0 };
    if loading.ready_frames < READY_FRAMES {
        return;
    }
    for entity in &screens {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<StartupLoading>();
    commands.remove_resource::<LoadingPipelines>();
}

fn report_loading_pipelines(mut main_world: ResMut<MainWorld>, pipelines: Res<PipelineCache>) {
    let Some(mut loading) = main_world.get_resource_mut::<LoadingPipelines>() else {
        return;
    };
    loading.ready = pipelines.waiting_pipelines().next().is_none();
    // Missing shader dependencies are retried while a pipeline is waiting.
    loading.failed = loading.ready
        && pipelines
            .pipelines()
            .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Err(_)));
}
