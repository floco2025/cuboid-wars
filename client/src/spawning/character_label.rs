use bevy::{
    asset::RenderAssetUsages,
    camera::{ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::{constants::*, markers::*};

pub(super) fn setup_character_label_text_rendering(
    commands: &mut Commands,
    images: &mut ResMut<Assets<Image>>,
) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width: LABEL_TEXTURE_WIDTH,
        height: LABEL_TEXTURE_HEIGHT,
        ..default()
    };

    let bg = Color::NONE.to_srgba();
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[
            (bg.blue * 255.0) as u8,
            (bg.green * 255.0) as u8,
            (bg.red * 255.0) as u8,
            (bg.alpha * 255.0) as u8,
        ],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let image_handle = images.add(image);

    let text_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image_handle.clone().into()),
        ))
        .id();

    (image_handle, text_camera)
}

pub fn spawn_character_label_display(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    character: Entity,
    label: &str,
    image_handle: Handle<Image>,
    text_camera: Entity,
    character_height: f32,
    max_health: f32,
) -> (Entity, Entity) {
    const LABEL_HEIGHT: f32 = LABEL_WIDTH * (LABEL_TEXTURE_HEIGHT as f32 / LABEL_TEXTURE_WIDTH as f32);

    let text_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(LABEL_BACKGROUND_COLOR),
                ))
                .with_children(|label_background| {
                    label_background.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: LABEL_FONT_SIZE,
                            ..default()
                        },
                        TextColor(LABEL_TEXT_COLOR),
                        TextLayout::new_with_no_wrap(),
                        CharacterLabelTextMarker,
                    ));
                });
            parent
                .spawn((
                    Node {
                        width: Val::Px(HEALTH_BAR_FLOATING_PLAYER_WIDTH),
                        height: Val::Px(HEALTH_BAR_FLOATING_PLAYER_HEIGHT),
                        justify_content: JustifyContent::FlexStart,
                        ..default()
                    },
                    BackgroundColor(HEALTH_BAR_TRACK_COLOR),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        CharacterHealthBarFillMarker { character, max_health },
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(HEALTH_BAR_FILL_COLOR),
                    ));
                });
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_WIDTH, LABEL_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                character_height / 2.0 + LABEL_HEIGHT_ABOVE_PLAYER + LABEL_HEIGHT / 2.0,
                0.0,
            ),
        ))
        .id();

    (text_entity, mesh_entity)
}

pub fn spawn_character_health_display(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    character: Entity,
    image_handle: Handle<Image>,
    text_camera: Entity,
    character_height: f32,
    max_health: f32,
) -> (Entity, Entity) {
    const BAR_TEXTURE_WIDTH: f32 = HEALTH_BAR_FLOATING_ACTOR_WIDTH;
    const BAR_TEXTURE_HEIGHT: f32 = HEALTH_BAR_FLOATING_ACTOR_HEIGHT + 10.0;
    const BAR_MESH_WIDTH: f32 = HEALTH_BAR_FLOATING_ACTOR_MESH_WIDTH;
    const BAR_MESH_HEIGHT: f32 = BAR_MESH_WIDTH * (BAR_TEXTURE_HEIGHT / BAR_TEXTURE_WIDTH);

    let ui_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(HEALTH_BAR_FLOATING_ACTOR_WIDTH),
                        height: Val::Px(HEALTH_BAR_FLOATING_ACTOR_HEIGHT),
                        justify_content: JustifyContent::FlexStart,
                        ..default()
                    },
                    BackgroundColor(HEALTH_BAR_TRACK_COLOR),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        CharacterHealthBarFillMarker { character, max_health },
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(HEALTH_BAR_FILL_COLOR),
                    ));
                });
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(BAR_MESH_WIDTH, BAR_MESH_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                character_height / 2.0 + LABEL_HEIGHT_ABOVE_PLAYER + BAR_MESH_HEIGHT / 2.0,
                0.0,
            ),
        ))
        .id();

    (ui_entity, mesh_entity)
}
