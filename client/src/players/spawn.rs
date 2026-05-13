use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use super::BumpFlashState;
use crate::{
    animations::{AnimationToPlay, character_animation_system},
    characters::{PreviousTickPosition, spawn_collider_box},
    config::{AssetSet, RenderSettings},
    constants::{LABEL_PLAYER_TEXTURE_HEIGHT, LABEL_PLAYER_TEXTURE_WIDTH},
    ui::floating_labels::{LabelCamera, setup_label_texture, spawn_floating_player_label},
};
use common::{config::GameplayConfig, physics::CharacterVerticalVelocity, protocol::*};

// Marks the local-player entity (the player you control). Spawned by
// `spawn_player` when `is_local` is true; queried by input, camera, UI,
// and effect systems that should only act on the local player.
#[derive(Component)]
pub struct LocalPlayerMarker;

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct PlayerBundle {
    player_id: PlayerId,
    player_marker: PlayerMarker,
    position: Position,
    move_intent: PlayerMoveIntent,
    motion: CharacterVerticalVelocity,
    health: Health,
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
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    player_id: u32,
    player_name: &str,
    position: &Position,
    move_intent: PlayerMoveIntent,
    health: Health,
    face_dir: f32,
    is_local: bool,
) -> Entity {
    let player_model = asset_set.player_model();
    let player_physics = gameplay_config.player.physics();
    // Create animation graph for this player
    let (graph, index) = AnimationGraph::from_clip(
        asset_server.load(GltfAssetLabel::Animation(0).from_asset(player_model.scene.clone())),
    );
    let graph_handle = graphs.add(graph);
    let animation_to_play = AnimationToPlay {
        graph_handle,
        index,
        speed: player_model.animation_speed.unwrap_or(1.0),
    };

    let entity = commands
        .spawn((
            PlayerBundle {
                player_id: PlayerId(player_id),
                player_marker: PlayerMarker,
                position: *position,
                move_intent,
                motion: CharacterVerticalVelocity::default(),
                health,
                face_direction: FaceDirection(face_dir),
                transform: Transform::from_xyz(position.x, player_physics.collider_center_y(position.y), position.z)
                    .with_rotation(Quat::from_rotation_y(face_dir)),
                visibility: player_visibility(is_local),
            },
            PreviousTickPosition(*position),
            animation_to_play.clone(),
        ))
        .id();

    if is_local {
        commands
            .entity(entity)
            .insert((LocalPlayerMarker, BumpFlashState::default()));
    }

    let mut children = vec![];

    // Add transparent collider visualization if enabled.
    if render_settings.debug_collider_boxes {
        children.push(spawn_collider_box(commands, meshes, materials, player_physics));
    }

    // Add the GLB player model with animation observer
    let base_y = player_physics.model_y_offset_from_entity_center(player_model.y_offset);
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(player_model.scene.clone())),
            Transform::from_scale(Vec3::splat(player_model.scale)).with_translation(Vec3::new(
                player_model.x_offset,
                base_y,
                player_model.z_offset,
            )),
            animation_to_play,
        ))
        .observe(character_animation_system)
        .id();
    children.push(model);

    // Create individual texture and camera for this player's ID text
    let (image_handle, text_camera) = setup_label_texture(
        commands,
        images,
        LABEL_PLAYER_TEXTURE_WIDTH,
        LABEL_PLAYER_TEXTURE_HEIGHT,
    );
    let (_text_entity, mesh_entity) = spawn_floating_player_label(
        commands,
        meshes,
        materials,
        entity,
        player_name,
        image_handle,
        text_camera,
        player_physics.collision_height(),
        gameplay_config.player.health().max,
        health.0,
    );
    children.push(mesh_entity);

    // The visibility system reads this to find the player's label camera and
    // toggle it on/off based on distance to the main camera and health changes.
    commands.entity(entity).insert(LabelCamera(text_camera));
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
