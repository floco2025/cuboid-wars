use bevy::{
    asset::RenderAssetUsages,
    gltf::GltfAssetLabel,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    scene::SceneRoot,
};

use crate::{
    constants::*,
    markers::*,
    systems::{
        animations::{AnimationToPlay, players_animation_system},
        players::BumpFlashState,
    },
};
use common::{constants::*, markers::PlayerMarker, protocol::*};

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct PlayerBundle {
    player_id: PlayerId,
    player_marker: PlayerMarker,
    position: Position,
    velocity: Velocity,
    face_direction: FaceDirection,
    transform: Transform,
    visibility: Visibility,
}

// ============================================================================
// Player Spawning
// ============================================================================

// Spawn a player model plus cosmetic children, returning the new entity id.
pub fn spawn_player(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    player_id: u32,
    player_name: &str,
    position: &Position,
    velocity: Velocity,
    face_dir: f32,
    is_local: bool,
) -> Entity {
    // Create animation graph for this player
    let (graph, index) =
        AnimationGraph::from_clip(asset_server.load(GltfAssetLabel::Animation(0).from_asset(PLAYER_MODEL)));
    let graph_handle = graphs.add(graph);
    let animation_to_play = AnimationToPlay { graph_handle, index };

    let entity = commands
        .spawn((
            PlayerBundle {
                player_id: PlayerId(player_id),
                player_marker: PlayerMarker,
                position: *position,
                velocity,
                face_direction: FaceDirection(face_dir),
                transform: Transform::from_xyz(position.x, position.y + PLAYER_HEIGHT / 2.0, position.z)
                    .with_rotation(Quat::from_rotation_y(face_dir)),
                visibility: player_visibility(is_local),
            },
            animation_to_play.clone(),
        ))
        .id();

    if is_local {
        commands
            .entity(entity)
            .insert((LocalPlayerMarker, BumpFlashState::default()));
    }

    let mut children = vec![];

    // Add transparent cuboid debug visualization if enabled
    if PLAYER_BOUNDING_BOX {
        let debug_box = commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(0.5, 0.5, 0.5, 0.15),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                })),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            ))
            .id();
        children.push(debug_box);
    }

    // Add the GLB player model with animation observer
    let base_y = PLAYER_MODEL_HEIGHT_OFFSET - PLAYER_HEIGHT / 2.0;
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(PLAYER_MODEL)),
            Transform::from_scale(Vec3::splat(PLAYER_MODEL_SCALE)).with_translation(Vec3::new(0.0, base_y, 0.0)),
            animation_to_play,
            PlayerModelMarker,
        ))
        .observe(players_animation_system)
        .id();
    children.push(model);

    // Create individual texture and camera for this player's ID text
    let (image_handle, text_camera) = setup_player_id_text_rendering(commands, images);
    let (_text_entity, mesh_entity) =
        spawn_player_id_display(commands, meshes, materials, player_name, image_handle, text_camera);
    children.push(mesh_entity);

    commands.entity(entity).add_children(&children);

    entity
}

const fn player_visibility(is_local: bool) -> Visibility {
    if is_local {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

fn setup_player_id_text_rendering(
    commands: &mut Commands,
    images: &mut ResMut<Assets<Image>>,
) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width: LABEL_TEXTURE_WIDTH,
        height: LABEL_TEXTURE_HEIGHT,
        ..default()
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[
            (LABEL_BACKGROUND_COLOR[2] * 255.0) as u8, // B
            (LABEL_BACKGROUND_COLOR[1] * 255.0) as u8, // G
            (LABEL_BACKGROUND_COLOR[0] * 255.0) as u8, // R
            (LABEL_BACKGROUND_COLOR[3] * 255.0) as u8, // A
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
                target: bevy::camera::RenderTarget::Image(image_handle.clone().into()),
                clear_color: bevy::camera::ClearColorConfig::Custom(Color::srgba(
                    LABEL_BACKGROUND_COLOR[0],
                    LABEL_BACKGROUND_COLOR[1],
                    LABEL_BACKGROUND_COLOR[2],
                    LABEL_BACKGROUND_COLOR[3],
                )),
                ..default()
            },
        ))
        .id();

    (image_handle, text_camera)
}

pub fn spawn_player_id_display(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_name: &str,
    image_handle: Handle<Image>,
    text_camera: Entity,
) -> (Entity, Entity) {
    const LABEL_HEIGHT: f32 = LABEL_WIDTH * (LABEL_TEXTURE_HEIGHT as f32 / LABEL_TEXTURE_WIDTH as f32);

    // Create UI text that renders to texture
    let text_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(
                LABEL_BACKGROUND_COLOR[0],
                LABEL_BACKGROUND_COLOR[1],
                LABEL_BACKGROUND_COLOR[2],
                LABEL_BACKGROUND_COLOR[3],
            )),
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(player_name),
                TextFont {
                    font_size: LABEL_FONT_SIZE,
                    ..default()
                },
                TextColor(Color::srgba(
                    LABEL_TEXT_COLOR[0],
                    LABEL_TEXT_COLOR[1],
                    LABEL_TEXT_COLOR[2],
                    LABEL_TEXT_COLOR[3],
                )),
                TextLayout::new_with_no_wrap(),
                PlayerIdTextMarker,
            ));
        })
        .id();

    // Create 3D plane mesh with the rendered texture
    let mesh_entity = commands
        .spawn((
            PlayerIdTextMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_WIDTH, LABEL_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                PLAYER_HEIGHT / 2.0 + LABEL_HEIGHT_ABOVE_PLAYER + LABEL_HEIGHT / 2.0,
                0.0,
            ),
        ))
        .id();

    (text_entity, mesh_entity)
}
