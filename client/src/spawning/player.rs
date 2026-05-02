use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use super::player_label::{setup_player_id_text_rendering, spawn_player_id_display};
use super::spawn_collider_box;
use crate::{
    config::{AssetSet, RenderSettings},
    markers::*,
    systems::{AnimationToPlay, BumpFlashState, character_animation_system},
};
use common::{config::GameplayConfig, markers::PlayerMarker, physics::CharacterVerticalVelocity, protocol::*};

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
    face_dir: f32,
    is_local: bool,
) -> Entity {
    let player_model = asset_set.player_model();
    let player_physics = gameplay_config.characters.player.physics();
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
                face_direction: FaceDirection(face_dir),
                transform: Transform::from_xyz(position.x, player_physics.collider_center_y(position.y), position.z)
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

    // Add transparent collider visualization if enabled.
    if render_settings.debug_collider_boxes {
        children.push(spawn_collider_box(commands, meshes, materials, player_physics));
    }

    // Add the GLB player model with animation observer
    let base_y = player_physics.model_y_offset_from_entity_center(player_model.model_y_offset);
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(player_model.scene.clone())),
            Transform::from_scale(Vec3::splat(player_model.scale)).with_translation(Vec3::new(0.0, base_y, 0.0)),
            animation_to_play,
            CharacterModelMarker,
        ))
        .observe(character_animation_system)
        .id();
    children.push(model);

    // Create individual texture and camera for this player's ID text
    let (image_handle, text_camera) = setup_player_id_text_rendering(commands, images);
    let (_text_entity, mesh_entity) = spawn_player_id_display(
        commands,
        meshes,
        materials,
        player_name,
        image_handle,
        text_camera,
        player_physics.collision_height(),
    );
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
