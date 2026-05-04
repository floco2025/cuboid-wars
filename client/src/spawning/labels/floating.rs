use bevy::prelude::*;

use crate::{constants::*, spawning::spawn_health_bar};

// Marker for the text node inside a floating label's UI render target.
// Used today only for the player name; the actor bar has no text.
#[derive(Component)]
pub struct CharacterLabelTextMarker;

// Marker for the floating-label 3D quad in world space (the textured rectangle
// above a character). Queried by `character_label_billboard_system` to rotate
// the quad to face the main camera each frame.
#[derive(Component)]
pub struct CharacterLabelMeshMarker;

// Lives on the character entity (player or actor). Stores the Entity id of
// its dedicated label-texture camera so the visibility system can toggle the
// camera's `is_active` flag based on distance + Health change.
#[derive(Component)]
pub struct LabelCamera(pub Entity);

pub fn spawn_floating_player_label(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    character: Entity,
    label: &str,
    image_handle: Handle<Image>,
    text_camera: Entity,
    character_height: f32,
    max_health: f32,
    current_health: f32,
) -> (Entity, Entity) {
    const LABEL_HEIGHT: f32 =
        LABEL_PLAYER_MESH_WIDTH * (LABEL_PLAYER_TEXTURE_HEIGHT as f32 / LABEL_PLAYER_TEXTURE_WIDTH as f32);

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

            spawn_health_bar(
                parent,
                character,
                max_health,
                current_health,
                HEALTH_BAR_FLOATING_PLAYER_WIDTH,
                HEALTH_BAR_FLOATING_PLAYER_HEIGHT,
            );
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_PLAYER_MESH_WIDTH, LABEL_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                character_height / 2.0 + LABEL_HEIGHT_ABOVE_CHARACTER + LABEL_HEIGHT / 2.0,
                0.0,
            ),
        ))
        .id();

    (text_entity, mesh_entity)
}

pub fn spawn_floating_actor_health_bar(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    character: Entity,
    image_handle: Handle<Image>,
    text_camera: Entity,
    character_height: f32,
    max_health: f32,
    current_health: f32,
) -> (Entity, Entity) {
    // Texture dimensions match the visible bar exactly (no padding); see
    // LABEL_ACTOR_TEXTURE_*. The mesh aspect mirrors the texture, so every
    // texel maps near-1:1 to a fragment on the mesh.
    const BAR_MESH_HEIGHT: f32 =
        LABEL_ACTOR_MESH_WIDTH * (HEALTH_BAR_FLOATING_ACTOR_HEIGHT / HEALTH_BAR_FLOATING_ACTOR_WIDTH);

    // No wrapper Node — the bar fills the texture exactly, so we render the
    // health bar directly at the camera root.
    let ui_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            spawn_health_bar(
                parent,
                character,
                max_health,
                current_health,
                HEALTH_BAR_FLOATING_ACTOR_WIDTH,
                HEALTH_BAR_FLOATING_ACTOR_HEIGHT,
            );
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_ACTOR_MESH_WIDTH, BAR_MESH_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                character_height / 2.0 + LABEL_HEIGHT_ABOVE_CHARACTER + BAR_MESH_HEIGHT / 2.0,
                0.0,
            ),
        ))
        .id();

    (ui_entity, mesh_entity)
}
