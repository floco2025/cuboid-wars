use std::f32::consts::TAU;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use common::{physics::CharacterSupport, protocol::ActorMoveIntent};

use super::ActorAnimationVelocity;
use crate::{characters::CharacterModel, config::WheelModelDef, constants::*};

// The actor a wheeled model drives for; the rig is built when its scene is ready.
#[derive(Component)]
pub(crate) struct WheelModel {
    pub owner: Entity,
    pub wheels: WheelModelDef,
    pub scale: f32,
}

#[derive(Clone)]
pub(crate) struct WheelAnimationSource {
    owner: Entity,
    graph: Handle<AnimationGraph>,
    idle: AnimationNodeIndex,
    drive: AnimationNodeIndex,
    wheel_radius: f32,
    cycle_secs: f32,
}

impl WheelAnimationSource {
    // `None` when the GLB lacks a configured clip.
    fn from_clips(
        model: &WheelModel,
        clips: &[Handle<AnimationClip>],
        graphs: &mut Assets<AnimationGraph>,
    ) -> Option<Self> {
        let idle = clips.get(model.wheels.idle_animation)?.clone();
        let drive = clips.get(model.wheels.drive_animation)?.clone();
        let (graph, nodes) = AnimationGraph::from_clips([idle, drive]);
        Some(Self {
            owner: model.owner,
            graph: graphs.add(graph),
            idle: nodes[0],
            drive: nodes[1],
            wheel_radius: model.wheels.radius * model.scale,
            cycle_secs: model.wheels.drive_cycle_secs,
        })
    }
}

#[derive(Component)]
pub(crate) struct WheelAnimationPlayback {
    source: WheelAnimationSource,
}

pub(crate) fn wheel_animation_setup_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    models: Query<(&CharacterModel, &WheelModel)>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok((model, wheel_model)) = models.get(ready.entity) else {
        return;
    };
    let Some(source) = model
        .clips(&gltfs)
        .and_then(|clips| WheelAnimationSource::from_clips(wheel_model, clips, &mut graphs))
    else {
        error!(
            "{} lacks wheel clips {} and {}",
            model.scene(),
            wheel_model.wheels.idle_animation,
            wheel_model.wheels.drive_animation
        );
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        let Ok(mut player) = players.get_mut(child) else {
            continue;
        };
        player.play(source.idle).repeat();
        player.play(source.drive).repeat().set_speed(0.0);
        commands.entity(child).insert((
            AnimationGraphHandle(source.graph.clone()),
            WheelAnimationPlayback { source: source.clone() },
        ));
    }
}

pub(super) fn drive_speed(intent: ActorMoveIntent, velocity: Vec3, support: Option<CharacterSupport>) -> f32 {
    if matches!(support, Some(CharacterSupport::Airborne | CharacterSupport::Ladder)) {
        return 0.0;
    }
    let control = intent.to_horizontal_velocity();
    velocity.dot(control.normalize_or_zero()).clamp(0.0, control.length())
}

pub(crate) fn wheel_animation_update_system(
    owners: Query<(&ActorAnimationVelocity, &ActorMoveIntent, Option<&CharacterSupport>)>,
    mut rigs: Query<(&WheelAnimationPlayback, &mut AnimationPlayer)>,
) {
    for (playback, mut player) in &mut rigs {
        let Ok((velocity, intent, support)) = owners.get(playback.source.owner) else {
            continue;
        };
        let speed = drive_speed(*intent, velocity.0, support.copied());
        if let Some(active) = player.animation_mut(playback.source.drive) {
            active.set_speed(if speed > WHEEL_ANIMATION_STANDSTILL_SPEED {
                speed * playback.source.cycle_secs / (TAU * playback.source.wheel_radius)
            } else {
                0.0
            });
        }
    }
}
