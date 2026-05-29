use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};

use crate::{
    constants::{LABEL_ACTOR_MESH_WIDTH, LABEL_BACKGROUND_COLOR, LABEL_PLAYER_MESH_WIDTH, LABEL_TEXT_COLOR},
    ui::spawn_health_bar,
};

// Floating-label dimensions resolved from config at spawn time. Bundled so
// the spawn signatures stay short and any future label-dim knob lands as a
// new field here instead of yet another loose `f32` parameter.
#[derive(Clone, Copy)]
pub struct FloatingLabelDims {
    pub height_above_character: f32,
    pub health_bar_width: f32,
    pub health_bar_height: f32,
    pub font_size: f32,
}

// Marker for the text node inside a floating label's UI render target.
// Used today only for the player name; the actor bar has no text.
#[derive(Component)]
pub struct CharacterLabelTextMarker;

// Marker for the floating-label 3D quad in world space (the textured rectangle
// above a character). Queried by `floating_labels_billboard_system` to rotate
// the quad to face the main camera each frame.
#[derive(Component)]
pub struct CharacterLabelMeshMarker;

// Lives on the character entity (player or actor). Holds its dedicated
// label-texture `camera` (toggled by the visibility system on distance +
// Health change) and the `ui_root` of the render-target UI tree. Both are
// top-level entities, not children of the character, so the `on_remove` hook
// despawns them with the character — otherwise the camera (its live render
// pass + target image) and the UI tree leak on every actor death / logoff.
#[derive(Component, Clone, Copy)]
#[component(on_remove = despawn_label_render_target)]
pub struct LabelCamera {
    pub camera: Entity,
    pub ui_root: Entity,
}

fn despawn_label_render_target(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&LabelCamera { camera, ui_root }) = world.get::<LabelCamera>(ctx.entity) else {
        return;
    };
    let mut commands = world.commands();
    commands.entity(camera).try_despawn();
    commands.entity(ui_root).try_despawn();
}

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
    dims: FloatingLabelDims,
) -> (Entity, Entity) {
    use crate::constants::{LABEL_PLAYER_TEXTURE_HEIGHT, LABEL_PLAYER_TEXTURE_WIDTH};
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
                            font_size: dims.font_size,
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
                dims.health_bar_width,
                dims.health_bar_height,
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
                character_height / 2.0 + dims.height_above_character + LABEL_HEIGHT / 2.0,
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
    dims: FloatingLabelDims,
) -> (Entity, Entity) {
    // Texture dimensions match the visible bar exactly (no padding). The
    // mesh aspect mirrors the texture so every texel maps near-1:1.
    let bar_mesh_height = LABEL_ACTOR_MESH_WIDTH * (dims.health_bar_height / dims.health_bar_width);

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
                dims.health_bar_width,
                dims.health_bar_height,
            );
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_ACTOR_MESH_WIDTH, bar_mesh_height))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                character_height / 2.0 + dims.height_above_character + bar_mesh_height / 2.0,
                0.0,
            ),
        ))
        .id();

    (ui_entity, mesh_entity)
}
