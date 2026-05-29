use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use crate::{
    animations::{AnimationToPlay, character_animation_system},
    characters::{PreviousTickPosition, spawn_collider_box},
    config::{AssetSet, ClientSettings},
    ui::floating_labels::{FloatingLabelDims, LabelCamera, setup_label_texture, spawn_floating_actor_health_bar},
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
    client_settings: &ClientSettings,
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
    if client_settings.debug.collider_boxes {
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
        let (graph, index) = AnimationGraph::from_clip(
            asset_server
                .load(GltfAssetLabel::Animation(actor_model.animation_index).from_asset(actor_model.scene.clone())),
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

    // The actor's floating-label texture matches the health bar 1:1 (no
    // padding), so the texture dimensions track `health_bars.floating_actor_*`.
    let health_bars = client_settings.hud.health_bars;
    let (image_handle, text_camera) = setup_label_texture(
        commands,
        images,
        health_bars.floating_actor_width as u32,
        health_bars.floating_actor_height as u32,
    );
    let (ui_entity, mesh_entity) = spawn_floating_actor_health_bar(
        commands,
        meshes,
        materials,
        entity,
        image_handle,
        text_camera,
        actor_physics.collision_height(),
        actor_config.health().max,
        actor.health.0,
        FloatingLabelDims {
            height_above_character: client_settings.hud.floating_labels.height_above_character,
            health_bar_width: health_bars.floating_actor_width,
            health_bar_height: health_bars.floating_actor_height,
            font_size: client_settings.hud.font_sizes.floating_label,
        },
    );
    children.push(mesh_entity);

    // The visibility system reads `camera` to toggle the actor's label camera
    // on distance/health changes; `ui_root` is tracked so both despawn with
    // the actor (see the `LabelCamera` on_remove hook).
    commands.entity(entity).insert(LabelCamera {
        camera: text_camera,
        ui_root: ui_entity,
    });
    commands.entity(entity).add_children(&children);

    entity
}
