use std::time::Duration;

use bevy::app::TaskPoolPlugin;
use serde_json::json;

use super::*;
use crate::{
    audio::{AudioAnalysis, audio_plugin, explosion_playback_settings},
    test_fixtures,
};

#[test]
fn actor_sfx_master_scales_beams_and_explosions_without_changing_player_sounds() {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<AudioSource>()
        .insert_resource(
            serde_json::from_value::<AudioAnalysis>(json!({"version": 1, "sounds": {
                "sounds/test-sfx.wav": {"suggested_gain_db": -12.0}
            }}))
            .expect("audio analysis fixture rejected"),
        )
        .add_plugins(audio_plugin);
    let mut assets = test_fixtures::asset_set();
    assets.actors.sfx_volume_db = -6.0;
    assets.actors.movement_volume_db = 20.0;
    let mut audio_config = test_fixtures::client_settings().audio;
    audio_config.explosion_gain = 2.0;
    let server = app.world().resource::<AssetServer>();
    let sound = SoundDef {
        file: "sounds/test-sfx.wav".into(),
        volume_db: 3.0,
    };
    let beam = actor_sfx_playback(
        server,
        &sound,
        assets.actors.sfx_volume_db,
        PlaybackSettings::ONCE
            .with_spatial(true)
            .with_start_position(Duration::from_millis(250)),
    );
    let explosion = actor_sfx_playback(
        server,
        &sound,
        assets.actors.sfx_volume_db,
        explosion_playback_settings(&audio_config, None),
    );
    let player = sound_playback(server, &sound, PlaybackSettings::DESPAWN);
    let beam = app.world_mut().spawn(beam).id();
    let explosion = app.world_mut().spawn(explosion).id();
    let player = app.world_mut().spawn(player).id();
    for _ in 0..3 {
        app.update();
        for (entity, expected_gain) in [
            (beam, 10.0_f32.powf(-15.0 / 20.0)),
            (explosion, 2.0 * 10.0_f32.powf(-15.0 / 20.0)),
            (player, 10.0_f32.powf(-9.0 / 20.0)),
        ] {
            let playback = app
                .world()
                .get::<PlaybackSettings>(entity)
                .expect("sound playback missing");
            assert!((playback.volume.to_linear() - expected_gain).abs() < 0.00001);
        }
        let playback = app
            .world()
            .get::<PlaybackSettings>(beam)
            .expect("beam playback missing");
        assert!(playback.spatial);
        assert_eq!(playback.start_position, Some(Duration::from_millis(250)));
    }
}
