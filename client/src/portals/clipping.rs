use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    cameras::CameraViewMode,
    characters::CharacterModel,
    constants::PORTAL_VIEW_BLEND_SECS,
    materials::{PortalClipMaterial, portal_clip_material},
    players::{LocalPlayerMarker, PlayerAnimationPlayback},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{PortalFrame, PortalSet, character_movement_center, traverse_point, traverse_rotation},
    protocol::{PlayerMarker, PortalEnd, PortalPairId},
};

// A player's body as portals draw it: the model, its twin, hidden until the
// body straddles a portal plane and then drawn at the paired end, the
// model's resting transform, and the crossing state that keeps the two
// continuous.
#[derive(Component)]
pub struct PortalBody {
    pub model: Entity,
    pub twin: Entity,
    base: Transform,
    // The gate straddled last frame, to notice the handoff to its paired end.
    gate: Option<(PortalPairId, PortalEnd)>,
    // The model's world rotation last frame.
    rotation: Quat,
    pose: Option<PoseTransient>,
}

impl PortalBody {
    pub fn new(model: Entity, twin: Entity, base: Transform) -> Self {
        Self {
            model,
            twin,
            base,
            gate: None,
            rotation: base.rotation,
            pose: None,
        }
    }
}

// After a handoff the upright body starts in the pose the twin showed,
// mapped through the pair, and this turn decays to nothing.
struct PoseTransient {
    turn: Quat,
    timer: Timer,
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

#[derive(SystemParam)]
pub(crate) struct PortalBodyWorld<'w> {
    time: Res<'w, Time>,
    fixed_time: Res<'w, Time<Fixed>>,
    portal_set: Res<'w, PortalSet>,
    carriers: Res<'w, Carriers>,
    gameplay_config: Res<'w, GameplayConfig>,
    view_mode: Res<'w, CameraViewMode>,
}

#[derive(SystemParam)]
pub(crate) struct ClipMaterialAccess<'w, 's> {
    standard: Res<'w, Assets<StandardMaterial>>,
    clip: ResMut<'w, Assets<PortalClipMaterial>>,
    meshes: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>>,
    states: Query<'w, 's, &'static mut PortalClipMaterials>,
}

// A body halfway through an aperture is drawn on both sides of the plane:
// the model clipped to the front of the gate it is in, and the twin at the
// paired end, clipped to the front of that gate, with the model's animation
// state copied over so both advance identically this frame. The gate is
// judged where it is drawn, and the tick the body's centre crosses, the
// model takes over the twin's pose and turns upright about its centre. The
// first-person camera does not draw the local body, and its twin stays
// hidden with it.
pub(crate) fn portal_body_clipping_system(
    mut commands: Commands,
    world: PortalBodyWorld,
    mut materials: ClipMaterialAccess,
    mut players: Query<(&Transform, &mut PortalBody, Has<LocalPlayerMarker>), With<PlayerMarker>>,
    mut models: Query<&mut Transform, (With<CharacterModel>, Without<PortalTwinMarker>, Without<PlayerMarker>)>,
    mut twins: Query<(&mut Transform, &mut Visibility), (With<PortalTwinMarker>, Without<PlayerMarker>)>,
    children: Query<&Children>,
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
    let alpha = world.fixed_time.overstep_fraction();
    let physics = world.gameplay_config.player.physics();
    for (player_transform, mut body, is_local) in &mut players {
        let body_hidden = is_local && world.view_mode.is_first_person();
        let straddle = (!body_hidden)
            .then(|| {
                world
                    .portal_set
                    .straddled_gate(player_transform.translation, physics, &world.carriers, alpha)
            })
            .flatten();
        let base_world = player_transform.mul_transform(body.base);
        if let (Some((pair, end)), Some(gate)) = (body.gate, &straddle)
            && pair == gate.pair
            && end != gate.end
        {
            body.pose = Some(PoseTransient {
                turn: handoff_turn(&gate.exit, &gate.entry, body.rotation, base_world.rotation),
                timer: Timer::from_seconds(PORTAL_VIEW_BLEND_SECS, TimerMode::Once),
            });
        }
        body.gate = straddle.as_ref().map(|gate| (gate.pair, gate.end));
        body.rotation = base_world.rotation;

        let Ok(mut model_transform) = models.get_mut(body.model) else {
            continue;
        };
        let transient = body.pose.as_mut().map(|pose| {
            pose.timer.tick(world.time.delta());
            (pose.timer.is_finished(), pose.timer.fraction(), pose.turn)
        });
        let model_world = match transient {
            Some((false, fraction, turn)) => {
                let eased = fraction * fraction * (3.0 - 2.0 * fraction);
                let centre = character_movement_center(player_transform.translation.into(), physics);
                let model_world = pose_about(centre, Quat::IDENTITY.slerp(turn, 1.0 - eased), base_world);
                *model_transform =
                    GlobalTransform::from(model_world).reparented_to(&GlobalTransform::from(*player_transform));
                model_world
            }
            _ => {
                if transient.is_some() {
                    body.pose = None;
                }
                model_transform.set_if_neq(body.base);
                base_world
            }
        };

        let Some(gate) = straddle else {
            for model in [body.model, body.twin] {
                restore_materials(&mut commands, &mut materials, model);
            }
            if let Ok((_, mut visibility)) = twins.get_mut(body.twin) {
                visibility.set_if_neq(Visibility::Hidden);
            }
            continue;
        };
        let Ok((mut twin_transform, mut twin_visibility)) = twins.get_mut(body.twin) else {
            continue;
        };
        *twin_transform = twin_transform_for(player_transform, &model_world, &gate.entry, &gate.exit);
        twin_visibility.set_if_neq(Visibility::Inherited);
        clip_model(
            &mut commands,
            &children,
            &mut materials,
            body.model,
            clip_plane(&gate.entry),
        );
        clip_model(
            &mut commands,
            &children,
            &mut materials,
            body.twin,
            clip_plane(&gate.exit),
        );
        mirror_animation(&mut commands, &children, &sources, &mut mirrors, body.model, body.twin);
    }
}

// The half-space in front of a gate, as the clip material's uniform.
fn clip_plane(frame: &PortalFrame) -> Vec4 {
    frame.normal.extend(-frame.center.dot(frame.normal))
}

// The turn that carries last frame's world rotation through the pair over
// the body's rotation after the handoff.
fn handoff_turn(entry: &PortalFrame, exit: &PortalFrame, before: Quat, after: Quat) -> Quat {
    traverse_rotation(entry, exit) * before * after.inverse()
}

// `pose` turned by `turn` about `centre`.
fn pose_about(centre: Vec3, turn: Quat, pose: Transform) -> Transform {
    Transform::from_translation(centre)
        .mul_transform(Transform::from_rotation(turn))
        .mul_transform(Transform::from_translation(-centre))
        .mul_transform(pose)
}

// The twin's transform under the player entity: the model's world pose
// mapped through the pair, brought back into the player's frame.
fn twin_transform_for(
    player: &Transform,
    model_world: &Transform,
    entry: &PortalFrame,
    exit: &PortalFrame,
) -> Transform {
    let mapped = Transform {
        translation: traverse_point(entry, exit, model_world.translation),
        rotation: traverse_rotation(entry, exit) * model_world.rotation,
        scale: model_world.scale,
    };
    GlobalTransform::from(mapped).reparented_to(&GlobalTransform::from(*player))
}

fn clip_model(
    commands: &mut Commands,
    children: &Query<&Children>,
    materials: &mut ClipMaterialAccess,
    model: Entity,
    plane: Vec4,
) {
    if let Ok(state) = materials.states.get(model) {
        for handle in &state.clipped {
            if materials
                .clip
                .get(handle)
                .is_some_and(|material| material.extension.plane != plane)
                && let Some(mut material) = materials.clip.get_mut(handle)
            {
                material.extension.plane = plane;
            }
        }
        return;
    }
    let mut state = PortalClipMaterials::default();
    let mut by_base: HashMap<AssetId<StandardMaterial>, Handle<PortalClipMaterial>> = HashMap::new();
    for entity in children.iter_descendants(model) {
        let Ok(MeshMaterial3d(standard)) = materials.meshes.get(entity) else {
            continue;
        };
        let Some(base) = materials.standard.get(standard) else {
            continue;
        };
        let clipped = by_base
            .entry(standard.id())
            .or_insert_with(|| materials.clip.add(portal_clip_material(base, plane)))
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

fn restore_materials(commands: &mut Commands, materials: &mut ClipMaterialAccess, model: Entity) {
    let Ok(state) = materials.states.get(model) else {
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
