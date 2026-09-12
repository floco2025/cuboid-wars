use bevy::prelude::*;

use super::{
    ActorAnimationVelocity,
    aim_rig::{FixedFacingMarker, aim_rig_setup_system},
    movement_audio::spawn_movement_audio,
    wheel_animation::{WheelModel, wheel_animation_setup_system},
    wheel_grounding::WheelGrounding,
};

use crate::{
    characters::{
        AnimationToPlay, MaxHealth, character_animation_system, load_character_model, model_transform,
        spawn_character_bounds,
    },
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
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    gameplay_config: &GameplayConfig,
    max_health: &MaxHealth,
    actor_id: ActorId,
    actor: &Actor,
) -> Entity {
    let actor_model = asset_set.actor_model(&actor.kind);
    let actor_physics = gameplay_config.expect_actor(&actor.kind).physics();
    let entity = commands
        .spawn((
            actor_id,
            ActorMarker,
            actor.movement.pos,
            ActorAnimationVelocity::default(),
            actor.movement.support,
            actor.movement.move_intent,
            actor.health,
            FaceYaw(actor.movement.face_yaw),
            CharacterVerticalVelocity(actor.movement.vertical_velocity),
            Transform::from_xyz(actor.movement.pos.x, actor.movement.pos.y, actor.movement.pos.z)
                .with_rotation(Quat::from_rotation_y(actor.movement.face_yaw)),
            Visibility::Visible,
        ))
        .id();

    let mut children = vec![];
    if !actor_model.rotate_with_facing {
        commands.entity(entity).insert(FixedFacingMarker);
    }
    children.push(spawn_character_bounds(commands, meshes, materials, actor_physics));

    let rest = model_transform(actor_model);
    let mut model_commands = commands.spawn((load_character_model(actor_model, asset_server), rest));

    if let Some(rig) = &actor_model.aim_rig {
        model_commands.insert(rig.clone()).observe(aim_rig_setup_system);
    }

    if let Some(wheels) = actor_model.wheels {
        model_commands
            .insert(WheelGrounding {
                owner: entity,
                physics: actor_physics,
                wheels,
                rest,
            })
            .insert(WheelModel {
                owner: entity,
                wheels,
                scale: actor_model.scale,
            })
            .observe(wheel_animation_setup_system);
    } else if let Some(animation_speed) = actor_model.animation_speed {
        model_commands
            .insert(AnimationToPlay {
                animation_index: actor_model.animation_index,
                speed: animation_speed,
            })
            .observe(character_animation_system);
    }

    children.push(model_commands.id());

    let health_bars = client_settings.hud.health_bars;
    let bar_width = LABEL_ACTOR_MESH_WIDTH;
    let bar_height = bar_width * health_bars.actor_aspect;
    let bar_y =
        actor_physics.hitbox.top_y_offset() + client_settings.hud.floating_labels.height_above + bar_height / 2.0;
    let bar_entity = spawn_floating_health_bar(
        commands,
        meshes,
        materials,
        entity,
        bar_width,
        bar_height,
        bar_y,
        max_health.actor(&actor.kind),
        actor.health.0,
    );
    children.push(bar_entity);

    commands.entity(entity).add_children(&children);

    if let Some(sound) = asset_set.actor_sound(&actor.kind, "movement") {
        spawn_movement_audio(
            commands,
            asset_server,
            entity,
            actor_id,
            gameplay_config.expect_actor(&actor.kind),
            sound,
            &client_settings.audio,
        );
    }

    entity
}

// Fade state for a ghost, straight from the wire ticks. Re-inserted onto
// an existing ghost on every snapshot, since `/spawn` moves the due tick.
#[must_use]
pub fn beam_in_ghost_state(gameplay_config: &GameplayConfig, spawning: &SpawningActor) -> BeamInGhost {
    let collider = gameplay_config.expect_actor(&spawning.kind).physics().hitbox;
    BeamInGhost {
        reserved_tick: spawning.reserved_tick,
        due_tick: spawning.due_tick,
        half_extents: Vec3::new(collider.width, collider.height, collider.depth) / 2.0,
        center_height: collider.center_y_offset(),
    }
}

// Beam-in ghost: the per-kind model at the reserved spot, fading in and
// sparkling for the warning window. No gameplay components — the actor
// doesn't exist yet; this is pure presentation. The spot is in the
// carrier's frame, so the ghost hangs under its carrier's entity like an
// item and rides with it.
pub fn spawn_actor_ghost(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    gameplay_config: &GameplayConfig,
    carrier: Entity,
    spawning: &SpawningActor,
) -> Entity {
    let actor_model = asset_set.actor_model(&spawning.kind);
    let ghost = beam_in_ghost_state(gameplay_config, spawning);
    let center_height = ghost.center_height;

    let entity = commands
        .spawn((
            ChildOf(carrier),
            Transform::from_xyz(spawning.pos.x, spawning.pos.y, spawning.pos.z)
                .with_rotation(Quat::from_rotation_y(spawning.face_yaw)),
            Visibility::Visible,
            ghost,
            BeamEmitter::default(),
        ))
        .id();

    commands.spawn((
        ChildOf(entity),
        Transform::from_xyz(0.0, center_height, 0.0),
        PointLight {
            color: BEAM_IN_COLOR,
            // Starts dark; the fade system ramps it with the window.
            intensity: 0.0,
            range: BEAM_IN_LIGHT_RANGE,
            shadow_maps_enabled: false,
            ..default()
        },
    ));

    let model = commands
        .spawn((
            load_character_model(actor_model, asset_server),
            model_transform(actor_model),
        ))
        // Once the scene hierarchy exists, swap in per-ghost translucent
        // material clones the fade system can drive.
        .observe(ghost_fade_setup_system)
        .id();
    commands.entity(entity).add_children(&[model]);

    entity
}
