use bevy::{animation::AnimationTargetId, prelude::*};
use common::{
    physics::CharacterSupport,
    protocol::{ActorMoveIntent, Position},
};

use super::wheel_animation::{
    WheelAnimationPlayback, WheelModel, drive_speed, wheel_animation_setup_system, wheel_animation_update_system,
};
use crate::{
    characters::{PreviousTickPosition, load_character_model},
    config::{ModelDef, WheelModelDef},
    test_assets::{headless_asset_app, settle},
};

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
    let server = app.world().resource::<AssetServer>().clone();
    app.world_mut()
        .spawn((
            load_character_model(&model, &server),
            WheelModel {
                owner,
                wheels,
                scale: model.scale,
            },
        ))
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
