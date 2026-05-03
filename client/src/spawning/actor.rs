use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use super::{
    character_label::{setup_character_label_text_rendering, spawn_character_health_display},
    spawn_collider_box,
};
use crate::{
    config::{AssetSet, RenderSettings},
    markers::CharacterModelMarker,
    systems::{AnimationToPlay, character_animation_system},
};
use common::{
    config::GameplayConfig,
    markers::ActorMarker,
    physics::CharacterVerticalVelocity,
    protocol::{Actor, ActorId, FaceDirection},
};

pub fn spawn_actor(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    actor_id: ActorId,
    actor: &Actor,
) -> Entity {
    let actor_model = asset_set.actor_model();
    let actor_physics = gameplay_config.characters.actor.physics();
    let entity = commands
        .spawn((
            actor_id,
            ActorMarker,
            actor.movement.pos,
            actor.movement.move_intent,
            actor.health,
            FaceDirection(actor.face_dir),
            CharacterVerticalVelocity(actor.movement.vertical_velocity),
            Transform::from_xyz(
                actor.movement.pos.x,
                actor_physics.collider_center_y(actor.movement.pos.y),
                actor.movement.pos.z,
            )
            .with_rotation(Quat::from_rotation_y(actor.face_dir)),
            Visibility::Visible,
        ))
        .id();

    let mut children = vec![];
    if render_settings.debug_collider_boxes {
        children.push(spawn_collider_box(commands, meshes, materials, actor_physics));
    }

    let base_y = actor_physics.model_y_offset_from_entity_center(actor_model.model_y_offset);
    let mut model_commands = commands.spawn((
        SceneRoot(asset_server.load(actor_model.scene.clone())),
        Transform::from_scale(Vec3::splat(actor_model.scale)).with_translation(Vec3::new(0.0, base_y, 0.0)),
        CharacterModelMarker,
    ));

    if let Some(animation_speed) = actor_model.animation_speed {
        let (graph, index) = AnimationGraph::from_clip(
            asset_server.load(GltfAssetLabel::Animation(0).from_asset(actor_model.scene.clone())),
        );
        model_commands
            .insert(AnimationToPlay {
                graph_handle: graphs.add(graph),
                index,
                speed: animation_speed,
            })
            .observe(character_animation_system);
    }

    children.push(model_commands.id());

    let (image_handle, text_camera) = setup_character_label_text_rendering(commands, images);
    let (_ui_entity, mesh_entity) = spawn_character_health_display(
        commands,
        meshes,
        materials,
        entity,
        image_handle,
        text_camera,
        actor_physics.collision_height(),
        gameplay_config.characters.actor.health().max,
    );
    children.push(mesh_entity);

    commands.entity(entity).add_children(&children);

    entity
}
