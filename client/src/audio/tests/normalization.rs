use std::time::Duration;

use bevy::{app::TaskPoolPlugin, audio::PlaybackMode};
use serde_json::{from_value, json};

use super::*;
use crate::{
    audio::sound_playback,
    config::AssetSet,
    test_fixtures,
    vfx::{RainIntensity, rain_audio_system},
};

fn audio_app() -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
    app.init_asset::<AudioSource>();
    let mut config: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/assets.json")).expect("asset fixture rejected");
    config["player"]["sounds"]["rain"] = json!({"file": "sounds/loud.ogg", "volume_db": 3.0});
    config["player"]["sounds"]["fall_damage"] = json!({"file": "sounds/loud.ogg", "volume_db": -2.0});
    config["player"]["sounds"]["landing"] = json!({"file": "sounds/loud.ogg", "volume_db": -12.0});
    config["player"]["sounds"]["fire"] = json!({"file": "sounds/quiet.ogg", "volume_db": -2.0});
    config["player"]["sounds"]["take_hit"] = json!({"file": "sounds/unknown.ogg"});
    let assets: AssetSet = from_value(config).expect("sound adjustment fixture rejected");
    let analysis: AudioAnalysis = from_value(json!({"version": 1, "sounds": {
        "sounds/loud.ogg": {"suggested_gain_db": -12.0},
        "sounds/quiet.ogg": {"suggested_gain_db": 6.0}
    }}))
    .expect("analysis fixture rejected");
    app.insert_resource(assets)
        .insert_resource(analysis)
        .add_plugins(audio_plugin);
    app
}

#[test]
fn sound_definitions_sharing_a_file_apply_independent_adjustments_once() {
    let mut app = audio_app();
    for (name, expected_db) in [
        ("rain", -15.0),
        ("fall_damage", -20.0),
        ("landing", -30.0),
        ("fire", -2.0),
        ("take_hit", -6.0),
    ] {
        for spatial in [false, true] {
            let bundle = sound_playback(
                app.world().resource::<AssetServer>(),
                app.world().resource::<AssetSet>().player_sound(name),
                PlaybackSettings::LOOP
                    .with_spatial(spatial)
                    .with_speed(0.75)
                    .with_start_position(Duration::from_millis(100))
                    .with_volume(Volume::Decibels(-6.0)),
            );
            let entity = app.world_mut().spawn(bundle).id();
            for _ in 0..3 {
                let playback = app
                    .world()
                    .get::<PlaybackSettings>(entity)
                    .expect("playback settings missing");
                assert!((playback.volume.to_decibels() - expected_db).abs() < 0.0001);
                assert_eq!(playback.spatial, spatial);
                assert!(matches!(playback.mode, PlaybackMode::Loop));
                assert_eq!(playback.speed, 0.75);
                assert_eq!(playback.start_position, Some(Duration::from_millis(100)));
                app.update();
            }
        }
    }
}

#[test]
fn samples_receive_only_normalization_and_muted_playback_stays_silent() {
    let mut app = audio_app();
    let handle = app.world().resource::<AssetServer>().load("sounds/loud.ogg");
    let entity = app.world_mut().spawn(AudioPlayer::new(handle)).id();
    let volume = app
        .world()
        .get::<PlaybackSettings>(entity)
        .expect("default playback settings missing")
        .volume;
    assert!((volume.to_decibels() + 12.0).abs() < 0.0001);

    let handle = app.world().resource::<AssetServer>().load("sounds/quiet.ogg");
    let entity = app
        .world_mut()
        .spawn((
            AudioPlayer::new(handle),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        ))
        .id();
    assert_eq!(
        app.world()
            .get::<PlaybackSettings>(entity)
            .expect("muted playback settings missing")
            .volume
            .to_linear(),
        0.0
    );
}

#[test]
fn rain_intensity_updates_keep_normalization_and_config_gain_without_reapplying_them() {
    let mut app = audio_app();
    let mut settings = test_fixtures::client_settings();
    settings.audio.rain_volume = 0.5;
    app.insert_resource(settings)
        .insert_resource(RainIntensity::default())
        .init_resource::<GlobalVolume>()
        .add_systems(Update, rain_audio_system);
    for intensity in [0.25, 0.5, 1.0, 0.5, 0.0, 1.0] {
        app.world_mut().resource_mut::<RainIntensity>().current = intensity;
        app.update();
        let world = app.world_mut();
        let mut sounds = world.query::<&PlaybackSettings>();
        if intensity == 0.0 {
            assert_eq!(sounds.iter(world).count(), 0);
        } else {
            let playback = sounds.single(world).expect("rain loop missing");
            let expected = intensity * 0.5 * 10.0_f32.powf(-9.0 / 20.0);
            assert!((playback.volume.to_linear() - expected).abs() < 0.00001);
        }
    }
}
