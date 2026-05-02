use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use super::player::spawn_collider_debug_box;
use crate::{
    config::{AssetSet, RenderSettings},
    markers::PlayerModelMarker,
    systems::{AnimationToPlay, players_animation_system},
};
use common::{
    config::GameplayConfig,
    markers::ActorMarker,
    physics::CharacterVerticalMotion,
    protocol::{Actor, ActorId, FaceDirection},
};

pub fn spawn_actor(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    _images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    actor_id: ActorId,
    actor: &Actor,
) -> Entity {
    let actor_model = asset_set.actor_model();
    let actor_collider = gameplay_config.characters.actor.collider;
    let entity = commands
        .spawn((
            actor_id,
            ActorMarker,
            actor.movement.pos,
            actor.movement.move_intent,
            FaceDirection(actor.face_dir),
            CharacterVerticalMotion(actor.movement.vertical_velocity),
            Transform::from_xyz(
                actor.movement.pos.x,
                actor.movement.pos.y + actor_collider.height / 2.0,
                actor.movement.pos.z,
            )
            .with_rotation(Quat::from_rotation_y(actor.face_dir)),
            Visibility::Visible,
        ))
        .id();

    let mut children = vec![];
    if render_settings.debug_bounding_boxes {
        children.push(spawn_collider_debug_box(commands, meshes, materials, actor_collider));
    }

    let base_y = actor_model.visual_y_offset - actor_collider.height / 2.0;
    let mut model_commands = commands.spawn((
        SceneRoot(asset_server.load(actor_model.scene.clone())),
        Transform::from_scale(Vec3::splat(actor_model.scale)).with_translation(Vec3::new(0.0, base_y, 0.0)),
        PlayerModelMarker,
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
            .observe(players_animation_system);
    }

    children.push(model_commands.id());
    commands.entity(entity).add_children(&children);

    entity
}
