use std::{sync::Arc, time::Duration};

use bevy::app::TaskPoolPlugin;
use serde_json::{from_value, json};

use super::*;
use crate::{
    audio::{AudioAnalysis, audio_plugin},
    test_fixtures,
};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
    test_fixtures::init_audio_app(&mut app);
    app.insert_resource(
        from_value::<AudioAnalysis>(json!({"version": 1, "sounds": {}})).expect("analysis fixture rejected"),
    )
    .add_plugins(audio_plugin);
    app
}

#[test]
fn spatial_sounds_play_through_the_filter_and_flat_sounds_do_not() {
    let mut app = app();
    let source = app.world_mut().resource_mut::<Assets<AudioSource>>().add(AudioSource {
        bytes: Arc::from([0_u8; 4]),
    });
    let missing = app
        .world()
        .resource::<AssetServer>()
        .load::<AudioSource>("sounds/missing.ogg");
    let spatial = app
        .world_mut()
        .spawn((
            AudioPlayer(source.clone()),
            PlaybackSettings::ONCE.with_spatial(true),
            Transform::default(),
        ))
        .id();
    let flat = app
        .world_mut()
        .spawn((AudioPlayer(source.clone()), PlaybackSettings::ONCE))
        .id();
    let pending = app
        .world_mut()
        .spawn((
            AudioPlayer(missing),
            PlaybackSettings::ONCE.with_spatial(true),
            Transform::default(),
        ))
        .id();
    let looping = app
        .world_mut()
        .spawn((
            AudioPlayer(source.clone()),
            PlaybackSettings::LOOP
                .with_spatial(true)
                .with_start_position(Duration::from_millis(100)),
            Transform::default(),
        ))
        .id();
    app.update();
    let world = app.world();
    // Held paused for the first probe; the loop moved into the source.
    let playback = world.get::<PlaybackSettings>(looping).expect("loop playback missing");
    assert!(playback.paused && matches!(playback.mode, PlaybackMode::Once) && playback.start_position.is_none());
    assert!(world.get::<PlaybackSettings>(spatial).expect("playback missing").paused);
    assert!(
        !world
            .get::<PlaybackSettings>(flat)
            .expect("flat playback missing")
            .paused
    );
    let looping = world
        .get::<AudioPlayer<LowPassAudio<AudioSource>>>(looping)
        .expect("filtered loop missing");
    let sources = world.resource::<Assets<LowPassAudio<AudioSource>>>();
    assert!(sources.get(&looping.0).expect("filtered loop source missing").repeats());
    assert!(world.get::<AudioPlayer<AudioSource>>(spatial).is_none());
    let filtered = world
        .get::<AudioPlayer<LowPassAudio<AudioSource>>>(spatial)
        .expect("filtered player missing");
    world
        .get::<AudioOcclusion>(spatial)
        .expect("occlusion missing")
        .cutoff()
        .set(600.0);
    let asset = sources.get(&filtered.0).expect("filtered source missing");
    assert_eq!(asset.cutoff().get(), 600.0);
    assert!(!asset.repeats());
    assert!(world.get::<AudioPlayer<AudioSource>>(flat).is_some());
    assert!(world.get::<AudioOcclusion>(flat).is_none());
    assert!(world.get::<AudioPlayer<AudioSource>>(pending).is_none());
    assert!(world.get::<SpatialSound<AudioSource>>(pending).is_some());
    assert!(world.get::<AudioPlayer<LowPassAudio<AudioSource>>>(pending).is_none());
}
