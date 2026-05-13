use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use crate::{
    animations::{AnimationToPlay, character_animation_system},
    characters::{PreviousTickPosition, spawn_collider_box},
    config::{AssetSet, RenderSettings},
    constants::{LABEL_ACTOR_TEXTURE_HEIGHT, LABEL_ACTOR_TEXTURE_WIDTH},
    ui::floating_labels::{LabelCamera, setup_label_texture, spawn_floating_actor_health_bar},
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{Actor, ActorId, ActorMarker, FaceDirection},
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
    let actor_model = asset_set.actor_model(&actor.kind);
    let actor_config = gameplay_config
        .actor(&actor.kind)
        .expect("actor kind sent by server is in gameplay config");
    let actor_physics = actor_config.physics();
    let entity = commands
        .spawn((
            actor_id,
            ActorMarker,
            actor.movement.pos,
            PreviousTickPosition(actor.movement.pos),
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

    let base_y = actor_physics.model_y_offset_from_entity_center(actor_model.y_offset);
    let mut model_commands = commands.spawn((
        SceneRoot(asset_server.load(actor_model.scene.clone())),
        Transform::from_scale(Vec3::splat(actor_model.scale))
            .with_rotation(Quat::from_rotation_x(actor_model.x_rotation_degrees.to_radians()))
            .with_translation(Vec3::new(actor_model.x_offset, base_y, actor_model.z_offset)),
    ));

    if let Some(animation_speed) = actor_model.animation_speed {
        let (graph, index) = AnimationGraph::from_clip(asset_server.load(
            GltfAssetLabel::Animation(actor_model.animation_index).from_asset(actor_model.scene.clone()),
        ));
        model_commands
            .insert(AnimationToPlay {
                graph_handle: graphs.add(graph),
                index,
                speed: animation_speed,
            })
            .observe(character_animation_system);
    }

    children.push(model_commands.id());

    let (image_handle, text_camera) =
        setup_label_texture(commands, images, LABEL_ACTOR_TEXTURE_WIDTH, LABEL_ACTOR_TEXTURE_HEIGHT);
    let (_ui_entity, mesh_entity) = spawn_floating_actor_health_bar(
        commands,
        meshes,
        materials,
        entity,
        image_handle,
        text_camera,
        actor_physics.collision_height(),
        actor_config.health().max,
        actor.health.0,
    );
    children.push(mesh_entity);

    // The visibility system reads this to find the actor's label camera and
    // toggle it on/off based on distance to the main camera and health changes.
    commands.entity(entity).insert(LabelCamera(text_camera));
    commands.entity(entity).add_children(&children);

    entity
}
