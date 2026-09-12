use std::time::Duration;

use bevy::{app::TaskPoolPlugin, audio::PlaybackMode, ecs::system::RunSystemOnce};
use common::config::ActorLocomotion;
use serde_json::json;

use super::*;
use crate::{
    audio::{AudioAnalysis, audio_plugin},
    test_fixtures,
};

fn audio_app() -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default(), TransformPlugin));
    app.init_asset::<AudioSource>();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(1.0 / 60.0));
    app.insert_resource(time)
        .init_resource::<GlobalVolume>()
        .insert_resource(
            serde_json::from_value::<AudioAnalysis>(json!({"version": 1, "sounds": {
                "sounds/test-movement.wav": {"suggested_gain_db": -12.0}
            }}))
            .expect("audio analysis fixture rejected"),
        )
        .add_plugins(audio_plugin)
        .add_systems(Update, actor_movement_audio_system);
    app
}

fn spawn_sound(app: &mut App, flying: bool, immovable: bool) -> Entity {
    let mut config = test_fixtures::gameplay_config().expect_actor("scuttler").clone();
    config.locomotion = if flying {
        ActorLocomotion::Flying
    } else {
        ActorLocomotion::Ground
    };
    config.immovable = immovable;
    let actor = app
        .world_mut()
        .spawn((
            Transform::default(),
            ActorAnimationVelocity::default(),
            ActorMoveIntent::Idle,
            CharacterSupport::Ground,
        ))
        .id();
    app.world_mut()
        .run_system_once(move |mut commands: Commands, server: Res<AssetServer>| {
            spawn_movement_audio(
                &mut commands,
                &server,
                actor,
                ActorId(1),
                &config,
                &SoundDef {
                    file: "sounds/test-movement.wav".to_owned(),
                    volume_db: 3.0,
                },
                &test_fixtures::client_settings().audio,
            );
        })
        .expect("movement sound spawn failed");
    actor
}

fn loop_entity(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<ActorMovementAudio>>()
        .single(app.world())
        .expect("actor movement loop missing")
}

#[test]
fn tank_loop_fades_pauses_resumes_and_dies_with_its_actor() {
    let mut app = audio_app();
    let actor = spawn_sound(&mut app, false, false);
    let sound = loop_entity(&mut app);
    app.update();
    assert!(
        app.world()
            .get::<PlaybackSettings>(sound)
            .expect("playback missing")
            .paused
    );
    app.world_mut().entity_mut(actor).insert((
        ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 3.0,
        },
        ActorAnimationVelocity(Vec3::Z * 3.0),
        Transform::from_xyz(8.0, 2.0, -4.0),
    ));
    app.update();
    let start_volume = app
        .world()
        .get::<PlaybackSettings>(sound)
        .expect("playback missing")
        .volume
        .to_linear();
    for _ in 0..60 {
        app.update();
    }
    let playback = app.world().get::<PlaybackSettings>(sound).expect("playback missing");
    assert!(playback.spatial && !playback.paused);
    assert!(matches!(playback.mode, PlaybackMode::Loop));
    assert!(playback.volume.to_linear() > start_volume);
    let state = app
        .world()
        .get::<ActorMovementAudio>(sound)
        .expect("movement audio state missing");
    assert!((playback.volume.to_linear() - state.gain * 10.0_f32.powf(-9.0 / 20.0)).abs() < 1e-6);
    let emitter = app
        .world()
        .get::<GlobalTransform>(sound)
        .expect("sound world transform missing")
        .translation();
    assert_eq!(emitter.x, 8.0);
    assert_eq!(emitter.z, -4.0);
    assert!(emitter.y >= 2.0);

    app.world_mut()
        .entity_mut(actor)
        .insert(ActorAnimationVelocity::default());
    app.update();
    assert!(
        !app.world()
            .get::<PlaybackSettings>(sound)
            .expect("playback missing")
            .paused
    );
    for _ in 0..90 {
        app.update();
    }
    let playback = app.world().get::<PlaybackSettings>(sound).expect("playback missing");
    assert!(playback.paused);
    assert_eq!(playback.volume.to_linear(), 0.0);

    app.world_mut()
        .entity_mut(actor)
        .insert(ActorAnimationVelocity(Vec3::Z * 3.0));
    app.update();
    assert_eq!(loop_entity(&mut app), sound);
    assert!(
        !app.world()
            .get::<PlaybackSettings>(sound)
            .expect("playback missing")
            .paused
    );
    app.world_mut().entity_mut(actor).despawn();
    assert!(app.world().get_entity(sound).is_err());
}

#[test]
fn drone_hovers_then_revs_for_vertical_flight_and_immovable_actors_have_no_loop() {
    let mut app = audio_app();
    spawn_sound(&mut app, false, true);
    assert_eq!(
        app.world_mut().query::<&ActorMovementAudio>().iter(app.world()).count(),
        0
    );
    let actor = spawn_sound(&mut app, true, false);
    let sound = loop_entity(&mut app);
    for _ in 0..90 {
        app.update();
    }
    let hover = *app
        .world()
        .get::<PlaybackSettings>(sound)
        .expect("hover playback missing");
    assert!(!hover.paused && hover.volume.to_linear() > 0.0);
    app.world_mut().entity_mut(actor).insert((
        ActorMoveIntent::Flying {
            velocity: [0.0, 4.0, 0.0],
        },
        ActorAnimationVelocity(Vec3::Y * 4.0),
        CharacterSupport::Airborne,
    ));
    for _ in 0..90 {
        app.update();
    }
    let flying = app
        .world()
        .get::<PlaybackSettings>(sound)
        .expect("flight playback missing");
    assert!(flying.volume.to_linear() > hover.volume.to_linear());
    assert!(flying.speed > hover.speed);
}

#[test]
fn actors_keep_independent_loops_through_brief_movement_gaps_and_stops() {
    let mut app = audio_app();
    let actors = [
        spawn_sound(&mut app, false, false),
        spawn_sound(&mut app, false, false),
        spawn_sound(&mut app, true, false),
    ];
    for (index, actor) in actors.into_iter().enumerate() {
        let speed = 2.0 + index as f32;
        app.world_mut().entity_mut(actor).insert((
            ActorAnimationVelocity(Vec3::Z * speed),
            ActorMoveIntent::Moving { direction: 0.0, speed },
        ));
    }
    for _ in 0..90 {
        app.update();
    }
    let sounds: Vec<_> = app
        .world_mut()
        .query_filtered::<(Entity, &ChildOf, &PlaybackSettings), With<ActorMovementAudio>>()
        .iter(app.world())
        .map(|(entity, parent, playback)| {
            assert!(!playback.paused);
            (entity, parent.parent(), playback.volume.to_linear())
        })
        .collect();
    assert_eq!(sounds.len(), 3);

    for actor in actors[..2].iter().copied() {
        let motion = app
            .world()
            .get::<ActorAnimationVelocity>(actor)
            .expect("actor travel missing")
            .0;
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity::default());
        for _ in 0..6 {
            app.update();
        }
        for &(sound, _, volume) in &sounds {
            let playback = app
                .world()
                .get::<PlaybackSettings>(sound)
                .expect("movement playback missing");
            assert!(!playback.paused);
            assert!(
                playback.volume.to_linear() > volume * 0.9,
                "brief travel gap ducked a loop"
            );
        }
        app.world_mut().entity_mut(actor).insert(ActorAnimationVelocity(motion));
        app.update();
    }

    app.world_mut().entity_mut(actors[0]).insert(ActorMoveIntent::Idle);
    for _ in 0..120 {
        app.update();
    }
    for &(sound, actor, volume) in &sounds {
        let playback = app
            .world()
            .get::<PlaybackSettings>(sound)
            .expect("movement playback missing");
        assert_eq!(playback.paused, actor == actors[0]);
        if actor != actors[0] {
            assert!((playback.volume.to_linear() - volume).abs() < 0.0001);
        }
    }
    app.world_mut().entity_mut(actors[0]).despawn();
    app.update();
    for &(sound, actor, _) in &sounds {
        assert_eq!(app.world().get_entity(sound).is_ok(), actor != actors[0]);
    }
}

#[test]
fn movement_audio_counts_climbing_but_excludes_idle_knockback_blocking_and_falls() {
    let moving = ActorMoveIntent::Moving {
        direction: 0.0,
        speed: 4.0,
    };
    assert_eq!(movement_speed(moving, Vec3::Z * 2.0, CharacterSupport::Ground), 2.0);
    assert_eq!(movement_speed(moving, Vec3::ZERO, CharacterSupport::Ground), 0.0);
    assert_eq!(movement_speed(moving, Vec3::X * 10.0, CharacterSupport::Ground), 0.0);
    assert_eq!(
        movement_speed(ActorMoveIntent::Idle, Vec3::Z * 10.0, CharacterSupport::Ground),
        0.0
    );
    assert_eq!(
        movement_speed(moving, Vec3::new(0.0, -5.0, 4.0), CharacterSupport::Airborne),
        0.0
    );
    for y in [-2.0, 2.0] {
        assert_eq!(
            movement_speed(
                ActorMoveIntent::Climbing {
                    direction: 0.0,
                    speed: 4.0
                },
                Vec3::Y * y,
                CharacterSupport::Ladder,
            ),
            2.0
        );
    }
    assert_eq!(
        movement_speed(
            ActorMoveIntent::ExitingLadder {
                direction: 0.0,
                speed: 4.0
            },
            Vec3::Z * 2.0,
            CharacterSupport::Ladder,
        ),
        2.0
    );
}
