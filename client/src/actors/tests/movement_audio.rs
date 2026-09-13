use bevy::{app::TaskPoolPlugin, audio::PlaybackMode, ecs::system::RunSystemOnce};
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
    let config = test_fixtures::gameplay_config().expect_actor(kind).clone();
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

#[test]
fn loop_follows_actor_and_pauses_without_modifying_the_recording() {
    for kind in ["scuttler", "bruiser", "zapper"] {
        let mut app = audio_app();
        let actor = spawn_sound(&mut app, kind);
        let sound = app
            .world_mut()
            .query_filtered::<Entity, With<ActorMovementAudioMarker>>()
            .single(app.world())
            .expect("movement loop missing");
        for velocity in [
            Vec3::ZERO,
            Vec3::Z * 3.0,
            Vec3::X * 8.0,
            -Vec3::Z * 0.1,
            Vec3::Y * 4.0,
            Vec3::ZERO,
            -Vec3::Y,
        ] {
            app.world_mut()
                .entity_mut(actor)
                .insert((ActorAnimationVelocity(velocity), Transform::from_translation(velocity)));
            app.update();
            let playback = app
                .world()
                .get::<PlaybackSettings>(sound)
                .expect("movement playback missing");
            assert!(playback.spatial);
            assert!(matches!(playback.mode, PlaybackMode::Once));
            assert_eq!(playback.paused, velocity == Vec3::ZERO);
            assert_eq!(playback.speed, 1.0);
            assert!((playback.volume.to_linear() - 10.0_f32.powf(-15.0 / 20.0)).abs() < 1e-6);
            let emitter = app
                .world()
                .get::<GlobalTransform>(sound)
                .expect("sound transform missing")
                .translation();
            assert_eq!(emitter.x, velocity.x);
            assert_eq!(emitter.z, velocity.z);
            assert!(emitter.y > velocity.y);
        }
        app.world_mut().entity_mut(actor).despawn();
        assert!(app.world().get_entity(sound).is_err());
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
        .query_filtered::<(Entity, &ChildOf), With<ActorMovementAudioMarker>>()
        .iter(app.world())
        .map(|(entity, parent)| (entity, parent.parent()))
        .collect();
    assert_eq!(sounds.len(), 20);
    for &stopped in &actors {
        app.world_mut()
            .entity_mut(stopped)
            .insert(ActorAnimationVelocity::default());
        app.update();
        for &(sound, actor) in &sounds {
            let playback = app
                .world()
                .get::<PlaybackSettings>(sound)
                .expect("movement playback missing");
            assert_eq!(playback.paused, actor == stopped);
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
        app.world_mut()
            .query::<&ActorMovementAudioMarker>()
            .iter(app.world())
            .count(),
        0
    );
}
