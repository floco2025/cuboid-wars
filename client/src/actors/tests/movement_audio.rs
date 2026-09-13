use std::{num::NonZero, time::Duration};

use bevy::{app::TaskPoolPlugin, audio::PlaybackMode, ecs::system::RunSystemOnce};
use common::config::ActorLocomotion;
use rodio::{SpatialPlayer, mixer::mixer, source::SineWave};
use serde_json::json;

use super::*;
use crate::{
    audio::{AudioAnalysis, LoopAudio, audio_plugin},
    config::AssetSet,
    test_fixtures,
};

fn audio_app() -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default(), TransformPlugin));
    app.init_asset::<LoopAudio>();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(1.0 / 60.0));
    app.insert_resource(time)
        .insert_resource(test_fixtures::client_settings())
        .init_resource::<GlobalVolume>();
    let mut assets = test_fixtures::asset_set();
    assets.actors.movement_volume_db = -6.0;
    assets.actors.sfx_volume_db = 20.0;
    app.insert_resource(assets)
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

fn spawn_sound(app: &mut App, kind: &str) -> Entity {
    let mut config = test_fixtures::gameplay_config().expect_actor(kind).clone();
    config.locomotion = if kind == "zapper" {
        ActorLocomotion::Flying
    } else {
        ActorLocomotion::Ground
    };
    let actor = app
        .world_mut()
        .spawn((Transform::default(), ActorAnimationVelocity::default()))
        .id();
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands, server: Res<AssetServer>, assets: Res<AssetSet>| {
                spawn_movement_audio(
                    &mut commands,
                    &server,
                    actor,
                    ActorId(3),
                    &config,
                    &SoundDef {
                        file: "sounds/test-movement.wav".to_owned(),
                        volume_db: 3.0,
                    },
                    &test_fixtures::client_settings().audio,
                    assets.actors.movement_volume_db,
                );
            },
        )
        .expect("movement sound spawn failed");
    actor
}

fn settle(app: &mut App) {
    for _ in 0..120 {
        app.update();
    }
}

fn loop_entity(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<ActorMovementAudio>>()
        .single(app.world())
        .expect("movement loop missing")
}

fn playback(app: &App, sound: Entity) -> PlaybackSettings {
    *app.world()
        .get::<PlaybackSettings>(sound)
        .expect("movement playback missing")
}

#[test]
fn ground_loops_follow_turns_and_fade_without_restarting() {
    for kind in ["scuttler", "bruiser"] {
        let mut app = audio_app();
        let actor = spawn_sound(&mut app, kind);
        let sound = loop_entity(&mut app);
        app.update();
        assert!(playback(&app, sound).paused);
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity(Vec3::Z * 0.5));
        app.update();
        let starting = playback(&app, sound);
        assert!(!starting.paused);
        settle(&mut app);
        let slow = playback(&app, sound);
        assert!(slow.volume.to_linear() > starting.volume.to_linear());
        assert!(slow.speed < 1.0);
        app.world_mut().entity_mut(actor).insert((
            ActorAnimationVelocity(-Vec3::X * 0.5),
            Transform::from_xyz(8.0, 2.0, -4.0),
        ));
        settle(&mut app);
        let turned = playback(&app, sound);
        assert!((turned.volume.to_linear() - slow.volume.to_linear()).abs() < 1e-6);
        assert!((turned.speed - slow.speed).abs() < 1e-4);
        assert!(turned.spatial && !turned.paused);
        assert!(matches!(turned.mode, PlaybackMode::Once));
        let emitter = app
            .world()
            .get::<GlobalTransform>(sound)
            .expect("sound transform missing")
            .translation();
        assert_eq!(emitter.x, 8.0);
        assert_eq!(emitter.z, -4.0);
        assert!(emitter.y > 2.0);
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity(Vec3::Y * 5.0));
        settle(&mut app);
        let fast = playback(&app, sound);
        assert!(fast.speed > slow.speed);
        assert!(fast.volume.to_linear() > slow.volume.to_linear());
        assert!((fast.volume.to_linear() - 10.0_f32.powf(-15.0 / 20.0)).abs() < 1e-6);
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity::default());
        app.update();
        let stopping = playback(&app, sound);
        assert!(!stopping.paused && stopping.volume.to_linear() < fast.volume.to_linear());
        settle(&mut app);
        assert!(playback(&app, sound).paused);
        assert_eq!(playback(&app, sound).volume.to_linear(), 0.0);
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity(Vec3::Z * 5.0));
        settle(&mut app);
        assert_eq!(loop_entity(&mut app), sound);
        assert!(!playback(&app, sound).paused);
        app.world_mut().entity_mut(actor).despawn();
        assert!(app.world().get_entity(sound).is_err());
    }
}

#[test]
fn drone_keeps_hovering_and_revs_for_vertical_flight() {
    let mut app = audio_app();
    let actor = spawn_sound(&mut app, "zapper");
    let sound = loop_entity(&mut app);
    settle(&mut app);
    let hover = playback(&app, sound);
    assert!(!hover.paused && hover.volume.to_linear() > 0.0);
    app.world_mut()
        .entity_mut(actor)
        .insert(ActorAnimationVelocity(Vec3::Y * 4.0));
    settle(&mut app);
    let flying = playback(&app, sound);
    assert!(flying.speed > hover.speed);
    assert!(flying.volume.to_linear() > hover.volume.to_linear());
    app.world_mut()
        .entity_mut(actor)
        .insert(ActorAnimationVelocity::default());
    settle(&mut app);
    let stopped = playback(&app, sound);
    assert!(!stopped.paused);
    assert!((stopped.volume.to_linear() - hover.volume.to_linear()).abs() < 1e-6);
}

#[test]
fn movement_volume_mutes_and_restores_live_loops_without_compounding_gain() {
    let mut app = audio_app();
    let actor = spawn_sound(&mut app, "bruiser");
    app.world_mut()
        .entity_mut(actor)
        .insert(ActorAnimationVelocity(Vec3::Z * 5.0));
    let sound = loop_entity(&mut app);
    let (input, _output) = mixer(
        NonZero::new(2).expect("zero channels"),
        NonZero::new(44100).expect("zero sample rate"),
    );
    let player = SpatialPlayer::connect_new(&input, [0.0; 3], [-0.015, 0.0, 0.0], [0.015, 0.0, 0.0]);
    player.append(SineWave::new(440.0));
    app.world_mut().entity_mut(sound).insert(SpatialAudioSink::new(player));
    settle(&mut app);
    let base = playback(&app, sound).volume.to_linear();
    for (db, master) in [
        (-20.0, 1.0),
        (-6.0, 0.5),
        (6.0, 1.0),
        (0.0, 0.0),
        (0.0, 1.0),
        (0.0, 1.0),
    ] {
        app.world_mut()
            .resource_mut::<ClientSettings>()
            .preferences
            .actor_movement_volume_db = db;
        app.world_mut().resource_mut::<GlobalVolume>().volume = Volume::Linear(master);
        settle(&mut app);
        let expected = base * if db == -20.0 { 0.0 } else { 10.0_f32.powf(db / 20.0) };
        let state = playback(&app, sound);
        assert!((state.volume.to_linear() - expected).abs() < 1e-6);
        let sink = app
            .world()
            .get::<SpatialAudioSink>(sound)
            .expect("live movement sink missing");
        assert!((sink.volume().to_linear() - expected * master).abs() < 1e-6);
        assert!(!sink.empty() && !sink.is_paused());
        assert_eq!(sink.speed(), state.speed);
        assert_eq!(loop_entity(&mut app), sound);
    }
}

#[test]
fn stopping_one_actor_leaves_the_other_loops_playing() {
    let mut app = audio_app();
    let actors: Vec<_> = (0..20)
        .map(|index| spawn_sound(&mut app, ["scuttler", "bruiser", "zapper"][index % 3]))
        .collect();
    for &actor in &actors {
        app.world_mut()
            .entity_mut(actor)
            .insert(ActorAnimationVelocity(Vec3::Z * 3.0));
    }
    app.update();
    let sounds: Vec<_> = app
        .world_mut()
        .query_filtered::<(Entity, &ChildOf), With<ActorMovementAudio>>()
        .iter(app.world())
        .map(|(entity, parent)| (entity, parent.parent()))
        .collect();
    assert_eq!(sounds.len(), 20);
    for &stopped in &actors {
        app.world_mut()
            .entity_mut(stopped)
            .insert(ActorAnimationVelocity::default());
        settle(&mut app);
        for &(sound, actor) in &sounds {
            let playback = app
                .world()
                .get::<PlaybackSettings>(sound)
                .expect("movement playback missing");
            let flying = app
                .world()
                .get::<ActorMovementAudio>(sound)
                .expect("movement state missing")
                .flying;
            assert_eq!(playback.paused, actor == stopped && !flying);
        }
        app.world_mut()
            .entity_mut(stopped)
            .insert(ActorAnimationVelocity(Vec3::X * 2.0));
    }
    app.world_mut().entity_mut(actors[0]).despawn();
    for (sound, actor) in sounds {
        assert_eq!(app.world().get_entity(sound).is_ok(), actor != actors[0]);
    }
}

#[test]
fn immovable_actor_has_no_movement_loop() {
    let mut app = audio_app();
    spawn_sound(&mut app, "turret");
    assert_eq!(
        app.world_mut().query::<&ActorMovementAudio>().iter(app.world()).count(),
        0
    );
}
