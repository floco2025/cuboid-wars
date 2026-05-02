use std::collections::HashSet;

use bevy::{
    gltf::GltfAssetLabel,
    light::NotShadowCaster,
    prelude::*,
    scene::{SceneInstance, SceneRoot, SceneSpawner},
};

use super::player_label::{setup_player_id_text_rendering, spawn_player_id_display};
use crate::{
    config::{AssetSet, ModelDef, RenderSettings},
    markers::*,
    systems::{AnimationToPlay, BumpFlashState, players_animation_system},
};
use common::{constants::*, markers::PlayerMarker, physics::CharacterVerticalMotion, protocol::*};

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct PlayerBundle {
    player_id: PlayerId,
    player_marker: PlayerMarker,
    position: Position,
    move_intent: CharacterMoveIntent,
    motion: CharacterVerticalMotion,
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
    player_id: u32,
    player_name: &str,
    position: &Position,
    move_intent: CharacterMoveIntent,
    face_dir: f32,
    is_local: bool,
) -> Entity {
    let player_model = asset_set.player_model();
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
                motion: CharacterVerticalMotion::default(),
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
    if render_settings.debug_bounding_boxes {
        children.push(spawn_model_bounding_box(commands, meshes, materials, player_model));
    }

    // Add the GLB player model with animation observer
    let base_y = player_model.height_offset - PLAYER_HEIGHT / 2.0;
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(player_model.scene.clone())),
            Transform::from_scale(Vec3::splat(player_model.scale)).with_translation(Vec3::new(0.0, base_y, 0.0)),
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

pub fn spawn_model_bounding_box(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    model: &ModelDef,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                model.bounding_box.width,
                model.bounding_box.height,
                model.bounding_box.depth,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.5, 0.5, 0.5, 0.15),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_translation(Vec3::ZERO),
        ))
        .id()
}

pub fn player_shadow_settings_system(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    scene_spawner: Res<SceneSpawner>,
    player_models: Query<(Entity, &SceneInstance), With<PlayerModelMarker>>,
    mut processed: Local<HashSet<Entity>>,
) {
    if render_settings.shadows_player_enabled {
        return;
    }

    for (model_entity, scene_instance) in &player_models {
        if processed.contains(&model_entity) || !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }

        for entity in scene_spawner.iter_instance_entities(**scene_instance) {
            commands.entity(entity).insert(NotShadowCaster);
        }
        processed.insert(model_entity);
    }
}

const fn player_visibility(is_local: bool) -> Visibility {
    if is_local {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}
