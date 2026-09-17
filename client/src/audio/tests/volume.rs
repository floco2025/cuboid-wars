use std::num::NonZero;

use rodio::{SpatialPlayer, mixer::mixer, source::SineWave};

use super::*;
use crate::constants::AUDIO_OCCLUSION_LAYER_GAIN;

#[test]
fn spatial_sinks_take_their_settings_the_master_volume_and_their_occlusion_then_play() {
    let mut app = App::new();
    app.init_resource::<GlobalVolume>()
        .add_systems(Last, spatial_sink_volume_system);
    let (input, _output) = mixer(
        NonZero::new(2).expect("zero channels"),
        NonZero::new(44100).expect("zero sample rate"),
    );
    let player = SpatialPlayer::connect_new(&input, [0.0; 3], [-0.015, 0.0, 0.0], [0.015, 0.0, 0.0]);
    player.append(SineWave::new(440.0));
    let mut occlusion = AudioOcclusion::holding();
    occlusion.probe(1.0);
    occlusion.ease(0.0);
    let sink = SpatialAudioSink::new(player);
    sink.pause();
    let sound = app
        .world_mut()
        .spawn((
            sink,
            PlaybackSettings::ONCE.with_volume(Volume::Linear(0.5)).paused(),
            occlusion,
        ))
        .id();
    let volume = |app: &App| {
        app.world()
            .get::<SpatialAudioSink>(sound)
            .expect("sink missing")
            .volume()
            .to_linear()
    };
    app.update();
    assert!((volume(&app) - 0.5 * AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
    let world = app.world();
    assert!(!world.get::<SpatialAudioSink>(sound).expect("sink missing").is_paused());
    assert!(!world.get::<PlaybackSettings>(sound).expect("playback missing").paused);
    app.world_mut().resource_mut::<GlobalVolume>().volume = Volume::Linear(0.5);
    app.update();
    assert!((volume(&app) - 0.25 * AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
}
