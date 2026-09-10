use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    cameras::CameraViewMode,
    characters::CharacterModel,
    materials::{PortalClipMaterial, portal_clip_material},
    players::{LocalPlayerMarker, PlayerAnimationPlayback},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{PortalFrame, PortalSet, traverse_point, traverse_rotation},
    protocol::PlayerMarker,
};

// The model a player's body is drawn with and its twin, hidden until the
// body straddles a portal plane and then drawn at the paired end.
#[derive(Component)]
pub struct PortalBodyModels {
    pub model: Entity,
    pub twin: Entity,
}

#[derive(Component)]
pub struct PortalTwinMarker;

// A model's meshes while they are clipped: the materials to restore and the
// clip materials whose plane follows the gate.
#[derive(Component, Default)]
pub(crate) struct PortalClipMaterials {
    restore: Vec<(Entity, Handle<StandardMaterial>)>,
    clipped: Vec<Handle<PortalClipMaterial>>,
}

// A body halfway through an aperture is drawn on both sides of the plane:
// the model clipped to the front of the gate it is in, and the twin at the
// paired end, clipped to the front of that gate, with the model's animation
// state copied over so both advance identically this frame. The
// first-person camera does not draw the local body, and its twin stays
// hidden with it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn portal_body_clipping_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    carriers: Res<Carriers>,
    fixed_time: Res<Time<Fixed>>,
    gameplay_config: Res<GameplayConfig>,
    view_mode: Res<CameraViewMode>,
    standard_materials: Res<Assets<StandardMaterial>>,
    mut clip_materials: ResMut<Assets<PortalClipMaterial>>,
    players: Query<(&Transform, &PortalBodyModels, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    models: Query<&Transform, (With<CharacterModel>, Without<PortalTwinMarker>)>,
    mut twins: Query<(&mut Transform, &mut Visibility), (With<PortalTwinMarker>, Without<PlayerMarker>)>,
    children: Query<&Children>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut clip_states: Query<&mut PortalClipMaterials>,
    sources: Query<(&AnimationPlayer, &AnimationTransitions, &AnimationGraphHandle), With<PlayerAnimationPlayback>>,
    mut mirrors: Query<
        (
            &mut AnimationPlayer,
            Option<&mut AnimationTransitions>,
            Option<&AnimationGraphHandle>,
        ),
        Without<PlayerAnimationPlayback>,
    >,
) {
    let alpha = fixed_time.overstep_fraction();
    let physics = gameplay_config.player.physics();
    for (player_transform, bodies, is_local) in &players {
        let body_hidden = is_local && view_mode.is_first_person();
        let straddle = if body_hidden {
            None
        } else {
            portal_set
                .straddled_gate(player_transform.translation, physics)
                .map(|(entry, exit)| {
                    (
                        PortalFrame::from_portal_between(entry, &carriers, alpha),
                        PortalFrame::from_portal_between(exit, &carriers, alpha),
                    )
                })
        };
        let Some((entry, exit)) = straddle else {
            for model in [bodies.model, bodies.twin] {
                restore_materials(&mut commands, &mut clip_states, model);
            }
            if let Ok((_, mut visibility)) = twins.get_mut(bodies.twin) {
                visibility.set_if_neq(Visibility::Hidden);
            }
            continue;
        };
        let Ok(model_transform) = models.get(bodies.model) else {
            continue;
        };
        let Ok((mut twin_transform, mut twin_visibility)) = twins.get_mut(bodies.twin) else {
            continue;
        };
        *twin_transform = twin_transform_for(player_transform, model_transform, &entry, &exit);
        twin_visibility.set_if_neq(Visibility::Inherited);
        let mut clip = |model, plane| {
            clip_model(
                &mut commands,
                &mut clip_states,
                &children,
                &mesh_materials,
                &standard_materials,
                &mut clip_materials,
                model,
                plane,
            );
        };
        clip(bodies.model, clip_plane(&entry));
        clip(bodies.twin, clip_plane(&exit));
        mirror_animation(
            &mut commands,
            &children,
            &sources,
            &mut mirrors,
            bodies.model,
            bodies.twin,
        );
    }
}

// The half-space in front of a gate, as the clip material's uniform.
fn clip_plane(frame: &PortalFrame) -> Vec4 {
    frame.normal.extend(-frame.center.dot(frame.normal))
}

// The twin's transform under the player entity: the model's world pose
// mapped through the pair, brought back into the player's frame.
fn twin_transform_for(player: &Transform, model: &Transform, entry: &PortalFrame, exit: &PortalFrame) -> Transform {
    let model_world = player.mul_transform(*model);
    let mapped = Transform {
        translation: traverse_point(entry, exit, model_world.translation),
        rotation: traverse_rotation(entry, exit) * model_world.rotation,
        scale: model_world.scale,
    };
    GlobalTransform::from(mapped).reparented_to(&GlobalTransform::from(*player))
}

#[allow(clippy::too_many_arguments)]
fn clip_model(
    commands: &mut Commands,
    clip_states: &mut Query<&mut PortalClipMaterials>,
    children: &Query<&Children>,
    mesh_materials: &Query<&MeshMaterial3d<StandardMaterial>>,
    standard_materials: &Assets<StandardMaterial>,
    clip_materials: &mut Assets<PortalClipMaterial>,
    model: Entity,
    plane: Vec4,
) {
    if let Ok(state) = clip_states.get_mut(model) {
        for handle in &state.clipped {
            if clip_materials
                .get(handle)
                .is_some_and(|material| material.extension.plane != plane)
                && let Some(mut material) = clip_materials.get_mut(handle)
            {
                material.extension.plane = plane;
            }
        }
        return;
    }
    let mut state = PortalClipMaterials::default();
    let mut by_base: HashMap<AssetId<StandardMaterial>, Handle<PortalClipMaterial>> = HashMap::new();
    for entity in children.iter_descendants(model) {
        let Ok(MeshMaterial3d(standard)) = mesh_materials.get(entity) else {
            continue;
        };
        let Some(base) = standard_materials.get(standard) else {
            continue;
        };
        let clipped = by_base
            .entry(standard.id())
            .or_insert_with(|| clip_materials.add(portal_clip_material(base, plane)))
            .clone();
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(clipped));
        state.restore.push((entity, standard.clone()));
    }
    // Nothing to clip until the scene has spawned; the next frame tries again.
    if state.restore.is_empty() {
        return;
    }
    state.clipped = by_base.into_values().collect();
    commands.entity(model).insert(state);
}

fn restore_materials(commands: &mut Commands, clip_states: &mut Query<&mut PortalClipMaterials>, model: Entity) {
    let Ok(state) = clip_states.get(model) else {
        return;
    };
    for (entity, standard) in &state.restore {
        if let Ok(mut mesh) = commands.get_entity(*entity) {
            mesh.remove::<MeshMaterial3d<PortalClipMaterial>>()
                .insert(MeshMaterial3d(standard.clone()));
        }
    }
    commands.entity(model).remove::<PortalClipMaterials>();
}

fn mirror_animation(
    commands: &mut Commands,
    children: &Query<&Children>,
    sources: &Query<(&AnimationPlayer, &AnimationTransitions, &AnimationGraphHandle), With<PlayerAnimationPlayback>>,
    mirrors: &mut Query<
        (
            &mut AnimationPlayer,
            Option<&mut AnimationTransitions>,
            Option<&AnimationGraphHandle>,
        ),
        Without<PlayerAnimationPlayback>,
    >,
    model: Entity,
    twin: Entity,
) {
    let Some((player, transitions, graph)) = children
        .iter_descendants(model)
        .find_map(|entity| sources.get(entity).ok())
    else {
        return;
    };
    let Some(twin_rig) = children.iter_descendants(twin).find(|entity| mirrors.contains(*entity)) else {
        return;
    };
    let Ok((mut twin_player, twin_transitions, twin_graph)) = mirrors.get_mut(twin_rig) else {
        return;
    };
    twin_player.clone_from(player);
    match twin_transitions {
        Some(mut twin_transitions) => twin_transitions.clone_from(transitions),
        None => {
            commands.entity(twin_rig).insert(transitions.clone());
        }
    }
    if twin_graph.is_none_or(|twin_graph| twin_graph.0 != graph.0) {
        commands.entity(twin_rig).insert(graph.clone());
    }
}

#[cfg(test)]
#[path = "tests/clipping.rs"]
mod tests;
