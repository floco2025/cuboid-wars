use bevy::{gltf::GltfAssetLabel, prelude::*};

use crate::{
    characters::{AnimationToPlay, character_animation_system},
    characters::{PreviousTickPosition, spawn_collider_box},
    config::{AssetSet, ClientSettings},
    constants::{BEAM_IN_COLOR, BEAM_IN_LIGHT_RANGE, LABEL_ACTOR_MESH_WIDTH},
    ui::floating_labels::spawn_floating_health_bar,
    vfx::{BeamEmitter, BeamInGhost, ghost_fade_setup_system},
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{Actor, ActorId, ActorMarker, FaceYaw, SpawningActor},
};

pub fn spawn_actor(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
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
        .expect("actor kind sent by server is missing from gameplay config");
    let actor_physics = actor_config.physics();
    let entity = commands
        .spawn((
            actor_id,
            ActorMarker,
            actor.movement.pos,
            PreviousTickPosition(actor.movement.pos),
            actor.movement.move_intent,
            actor.health,
            FaceYaw(actor.face_yaw),
            CharacterVerticalVelocity(actor.movement.vertical_velocity),
            Transform::from_xyz(
                actor.movement.pos.x,
                actor_physics.collider_center_y(actor.movement.pos.y),
                actor.movement.pos.z,
            )
            .with_rotation(Quat::from_rotation_y(actor.face_yaw)),
            Visibility::Visible,
        ))
        .id();

    let mut children = vec![];
    if client_settings.debug.collider_boxes {
        children.push(spawn_collider_box(commands, meshes, materials, actor_physics));
    }

    let base_y = actor_physics.model_y_offset_from_entity_center(actor_model.y_offset);
    let mut model_commands = commands.spawn((
        WorldAssetRoot(asset_server.load(actor_model.scene.clone())),
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

    // Floating health bar: plain billboarded geometry parented to the actor
    // (track + fill quads), not a render-target camera — see
    // `spawn_floating_health_bar`. It despawns with the actor.
    let health_bars = client_settings.hud.health_bars;
    let bar_width = LABEL_ACTOR_MESH_WIDTH;
    let bar_height = bar_width * health_bars.floating_actor_aspect;
    let bar_y = actor_physics.collision_height() / 2.0
        + client_settings.hud.floating_labels.height_above_character
        + bar_height / 2.0;
    let bar_entity = spawn_floating_health_bar(
        commands,
        meshes,
        materials,
        entity,
        bar_width,
        bar_height,
        bar_y,
        actor_config.health().max,
        actor.health.0,
    );
    children.push(bar_entity);

    commands.entity(entity).add_children(&children);

    entity
}

// Fade state for a ghost, straight from the wire fields. Also inserted onto
// existing ghosts on every snapshot, resyncing the locally-ticked fade.
#[must_use]
pub fn beam_in_ghost_state(gameplay_config: &GameplayConfig, spawning: &SpawningActor) -> BeamInGhost {
    let collider = gameplay_config
        .actor(&spawning.kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics()
        .collider;
    BeamInGhost {
        remaining_secs: spawning.remaining_secs,
        warning_secs: spawning.warning_secs,
        half_extents: Vec3::new(collider.width, collider.height, collider.depth) / 2.0,
    }
}

// Beam-in ghost: the per-kind model at the reserved spot, fading in and
// sparkling for the warning window. No gameplay components — the actor
// doesn't exist yet; this is pure presentation.
pub fn spawn_actor_ghost(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    gameplay_config: &GameplayConfig,
    spawning: &SpawningActor,
) -> Entity {
    let actor_model = asset_set.actor_model(&spawning.kind);
    let actor_physics = gameplay_config
        .actor(&spawning.kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();

    let entity = commands
        .spawn((
            Transform::from_xyz(
                spawning.pos.x,
                actor_physics.collider_center_y(spawning.pos.y),
                spawning.pos.z,
            )
            .with_rotation(Quat::from_rotation_y(spawning.face_yaw)),
            Visibility::Visible,
            beam_in_ghost_state(gameplay_config, spawning),
            BeamEmitter::default(),
            PointLight {
                color: BEAM_IN_COLOR,
                // Starts dark; the fade system ramps it with the window.
                intensity: 0.0,
                range: BEAM_IN_LIGHT_RANGE,
                shadow_maps_enabled: false,
                ..default()
            },
        ))
        .id();

    let base_y = actor_physics.model_y_offset_from_entity_center(actor_model.y_offset);
    let model = commands
        .spawn((
            WorldAssetRoot(asset_server.load(actor_model.scene.clone())),
            Transform::from_scale(Vec3::splat(actor_model.scale))
                .with_rotation(Quat::from_rotation_x(actor_model.x_rotation_degrees.to_radians()))
                .with_translation(Vec3::new(actor_model.x_offset, base_y, actor_model.z_offset)),
        ))
        // Once the scene hierarchy exists, swap in per-ghost translucent
        // material clones the fade system can drive.
        .observe(ghost_fade_setup_system)
        .id();
    commands.entity(entity).add_children(&[model]);

    entity
}
