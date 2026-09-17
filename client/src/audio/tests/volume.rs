use std::num::NonZero;

use rodio::{SpatialPlayer, mixer::mixer, source::SineWave};

use super::*;
use crate::constants::AUDIO_OCCLUSION_LAYER_GAIN;

#[test]
fn spatial_sinks_take_their_settings_the_master_volume_and_their_occlusion() {
    let mut app = App::new();
    app.init_resource::<GlobalVolume>()
        .add_systems(Last, spatial_sink_volume_system);
    let (input, _output) = mixer(
        NonZero::new(2).expect("zero channels"),
        NonZero::new(44100).expect("zero sample rate"),
    );
    let player = SpatialPlayer::connect_new(&input, [0.0; 3], [-0.015, 0.0, 0.0], [0.015, 0.0, 0.0]);
    player.append(SineWave::new(440.0));
    let mut occlusion = AudioOcclusion::default();
    occlusion.probe(1.0);
    occlusion.ease(0.0);
    let sound = app
        .world_mut()
        .spawn((
            SpatialAudioSink::new(player),
            PlaybackSettings::ONCE.with_volume(Volume::Linear(0.5)),
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
    app.world_mut().resource_mut::<GlobalVolume>().volume = Volume::Linear(0.5);
    app.update();
    assert!((volume(&app) - 0.25 * AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
}
