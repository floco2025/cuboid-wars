use std::f32::consts::TAU;

use bevy::{gltf::GltfAssetLabel, prelude::*, world_serialization::WorldInstanceReady};
use common::{
    physics::CharacterSupport,
    protocol::{ActorMoveIntent, Position},
};

use crate::{
    characters::PreviousTickPosition,
    config::{ModelDef, WheelModelDef},
    constants::*,
};

#[derive(Component, Clone)]
pub(crate) struct WheelAnimationSource {
    owner: Entity,
    graph: Handle<AnimationGraph>,
    idle: AnimationNodeIndex,
    drive: AnimationNodeIndex,
    wheel_radius: f32,
    cycle_secs: f32,
}

impl WheelAnimationSource {
    pub fn load(
        owner: Entity,
        model: &ModelDef,
        wheels: WheelModelDef,
        server: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> Self {
        let (graph, clips) = AnimationGraph::from_clips(
            [wheels.idle_animation, wheels.drive_animation]
                .map(|index| server.load(GltfAssetLabel::Animation(index).from_asset(model.scene.clone()))),
        );
        Self {
            owner,
            graph: graphs.add(graph),
            idle: clips[0],
            drive: clips[1],
            wheel_radius: wheels.radius * model.scale,
            cycle_secs: wheels.drive_cycle_secs,
        }
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
    sources: Query<&WheelAnimationSource>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok(source) = sources.get(ready.entity) else {
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

fn drive_speed(intent: ActorMoveIntent, displacement: Vec3, delta: f32, support: Option<CharacterSupport>) -> f32 {
    if matches!(support, Some(CharacterSupport::Airborne | CharacterSupport::Ladder)) {
        return 0.0;
    }
    let control = intent.to_horizontal_velocity();
    (displacement.dot(control.normalize_or_zero()) / delta).clamp(0.0, control.length())
}

pub(crate) fn wheel_animation_update_system(
    fixed: Res<Time<Fixed>>,
    owners: Query<(
        &Position,
        &PreviousTickPosition,
        &ActorMoveIntent,
        Option<&CharacterSupport>,
    )>,
    mut rigs: Query<(&WheelAnimationPlayback, &mut AnimationPlayer)>,
) {
    for (playback, mut player) in &mut rigs {
        let Ok((position, previous, intent, support)) = owners.get(playback.source.owner) else {
            continue;
        };
        let speed = drive_speed(
            *intent,
            Vec3::from(*position) - Vec3::from(previous.0),
            fixed.timestep().as_secs_f32(),
            support.copied(),
        );
        if let Some(active) = player.animation_mut(playback.source.drive) {
            active.set_speed(if speed > WHEEL_ANIMATION_STANDSTILL_SPEED {
                speed * playback.source.cycle_secs / (TAU * playback.source.wheel_radius)
            } else {
                0.0
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_assets::{gltf_path, headless_asset_app, preload_gltf, settle};
    use bevy::animation::AnimationTargetId;

    #[test]
    fn bevy_loads_configured_wheeled_models_and_starts_and_stops_their_wheels() {
        let assets: serde_json::Value = serde_json::from_str(include_str!("../../../config/client/assets.json"))
            .expect("client assets JSON is invalid");
        for actor in assets["actors"].as_object().expect("actor assets missing").values() {
            let model: ModelDef = serde_json::from_value(actor["model"].clone()).expect("actor model is invalid");
            if let Some(wheels) = model.wheels {
                check_wheel_playback(model, wheels);
            }
        }
    }

    fn check_wheel_playback(model: ModelDef, wheels: WheelModelDef) {
        let mut app = headless_asset_app(|app| {
            app.add_systems(Update, wheel_animation_update_system);
        });
        let owner = app
            .world_mut()
            .spawn((
                Position::default(),
                PreviousTickPosition(Position::default()),
                ActorMoveIntent::Idle,
                CharacterSupport::Ground,
            ))
            .id();
        preload_gltf(&mut app, &gltf_path(&model.scene));
        let server = app.world().resource::<AssetServer>().clone();
        let source = WheelAnimationSource::load(
            owner,
            &model,
            wheels,
            &server,
            &mut app.world_mut().resource_mut::<Assets<AnimationGraph>>(),
        );
        app.world_mut()
            .spawn((WorldAssetRoot(server.load(model.scene.clone())), source))
            .observe(wheel_animation_setup_system);
        settle(&mut app, |world| {
            world.query::<&WheelAnimationPlayback>().iter(world).next().is_some()
        });
        let start: Vec<_> = app
            .world_mut()
            .query::<(&AnimationTargetId, &Transform)>()
            .iter(app.world())
            .map(|(id, transform)| (*id, transform.rotation))
            .collect();
        let delta = app.world().resource::<Time<Fixed>>().timestep().as_secs_f32();
        app.world_mut().entity_mut(owner).insert((
            Position {
                x: 0.0,
                y: 0.0,
                z: delta * 2.0,
            },
            ActorMoveIntent::Moving {
                direction: 0.0,
                speed: 2.0,
            },
        ));
        for _ in 0..8 {
            app.update();
        }
        let (wheel, angle) = app
            .world_mut()
            .query::<(&AnimationTargetId, &Transform)>()
            .iter(app.world())
            .filter_map(|(id, transform)| {
                start
                    .iter()
                    .find(|(before, _)| before == id)
                    .map(|(_, rotation)| (*id, rotation.angle_between(transform.rotation)))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("animated wheel targets missing");
        assert!(
            (angle - 8.0 / 30.0 * 2.0 / (wheels.radius * model.scale)).abs() < 0.03,
            "wheel rotation did not match the travelled distance: {}",
            model.scene
        );
        let wheel_rotation = |world: &mut World| {
            world
                .query::<(&AnimationTargetId, &Transform)>()
                .iter(world)
                .find(|(id, _)| **id == wheel)
                .expect("animated wheel target missing")
                .1
                .rotation
        };
        app.world_mut().entity_mut(owner).insert(ActorMoveIntent::Idle);
        for _ in 0..8 {
            app.update();
        }
        let stopped = wheel_rotation(app.world_mut());
        for _ in 0..6 {
            app.update();
        }
        assert!(
            stopped.angle_between(wheel_rotation(app.world_mut())) < 0.001,
            "idle wheels kept turning: {}",
            model.scene
        );
    }

    #[test]
    fn wheels_follow_actual_controlled_travel_and_stop_when_blocked_or_airborne() {
        let moving = ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 4.0,
        };
        let direction = moving.to_horizontal_velocity().normalize();
        assert_eq!(
            drive_speed(moving, direction * 0.2, 0.1, Some(CharacterSupport::Ground)),
            2.0
        );
        assert_eq!(
            drive_speed(moving, direction * 10.0, 0.1, Some(CharacterSupport::Ground)),
            4.0
        );
        assert_eq!(
            drive_speed(moving, Vec3::ZERO, 0.1, Some(CharacterSupport::Ground)),
            0.0
        );
        assert_eq!(
            drive_speed(
                ActorMoveIntent::Idle,
                direction * 10.0,
                0.1,
                Some(CharacterSupport::Ground)
            ),
            0.0
        );
        assert_eq!(
            drive_speed(moving, direction, 0.1, Some(CharacterSupport::Airborne)),
            0.0
        );
    }
}
