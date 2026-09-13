use std::{f32::consts::TAU, fs, thread, time::Instant};

use bevy::{
    app::TaskPoolPlugin,
    audio::{AudioLoader, SpatialAudioSink},
};
use rodio::{SpatialPlayer, mixer::mixer};

use super::*;

fn wav(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let size = samples.len() as u32 * 2;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((size + 36).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend((channels * 2).to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(size.to_le_bytes());
    for sample in samples {
        bytes.extend(sample.to_le_bytes());
    }
    bytes
}

#[test]
fn looping_preserves_stereo_frames_and_independent_playback_positions() {
    let samples = [-16384, 8192, 16384, -8192];
    let audio = LoopAudio::decode(wav(&samples, 2, 44100)).expect("stereo loop rejected");
    let expected = [-0.5, 0.25, 0.5, -0.25];
    let mut first = audio.decoder();
    let mut second = audio.decoder();
    for index in 0..13 {
        assert_eq!(first.next(), Some(expected[index % 4]));
        assert_eq!(first.channels().get(), 2);
        assert_eq!(first.sample_rate().get(), 44100);
        assert_eq!(first.current_span_len(), None);
    }
    assert_eq!(second.next(), Some(expected[0]));
    assert_eq!(first.next(), Some(expected[1]));
    assert!(LoopAudio::decode(wav(&[], 1, 44100)).is_err());
}

#[test]
fn nearby_tone_stays_dominant_without_changing_output_channels() {
    let frequencies = [220.0, 700.0, 1800.0];
    let sources = frequencies.map(|frequency| {
        let samples: Vec<_> = (0..44100)
            .map(|frame| (6500.0 * (TAU * frequency * frame as f32 / 44100.0).sin()).round() as i16)
            .collect();
        LoopAudio::decode(wav(&samples, 1, 44100)).expect("tone loop rejected")
    });
    for channels in [2, 8] {
        for rate in [44100, 48000] {
            let (input, mut output) = mixer(
                NonZero::new(channels).expect("output channel count is zero"),
                NonZero::new(rate).expect("output sample rate is zero"),
            );
            let sinks: Vec<_> = (0..20)
                .map(|index| {
                    let distance = if index == 0 { 0.5 } else { 4.0 + index as f32 * 0.25 };
                    let offset = Vec3::Z * distance;
                    let player =
                        SpatialPlayer::connect_new(&input, offset.to_array(), [-0.015, 0.0, 0.0], [0.015, 0.0, 0.0]);
                    player.append(sources[index % 3].decoder());
                    (SpatialAudioSink::new(player), offset)
                })
                .collect();
            let mut ranges = [(f32::MAX, 0.0_f32); 3];
            let frames = rate / 10;
            for block in 0..24 {
                let listener = Vec3::new(block as f32 * 0.1, 0.0, 0.0);
                for (sink, offset) in &sinks {
                    sink.set_ears_position(listener - Vec3::X * 0.015, listener + Vec3::X * 0.015);
                    sink.set_emitter_position(listener + offset);
                }
                let mut quadratures = [(0.0_f32, 0.0_f32); 3];
                for frame in 0..frames {
                    let mut stereo = 0.0;
                    for channel in 0..channels {
                        let sample = output.next().expect("movement mix ended");
                        if channel < 2 {
                            stereo += sample * 0.5;
                        } else {
                            assert_eq!(sample, 0.0, "spatial loop shifted into output channel {channel}");
                        }
                    }
                    for (index, frequency) in frequencies.iter().enumerate() {
                        let angle = TAU * frequency * frame as f32 / rate as f32;
                        quadratures[index].0 += stereo * angle.cos();
                        quadratures[index].1 += stereo * angle.sin();
                    }
                }
                if block > 0 {
                    for (index, (real, imaginary)) in quadratures.into_iter().enumerate() {
                        let amplitude = real.hypot(imaginary) * 2.0 / frames as f32;
                        ranges[index].0 = ranges[index].0.min(amplitude);
                        ranges[index].1 = ranges[index].1.max(amplitude);
                    }
                }
            }
            for (minimum, maximum) in ranges {
                assert!(minimum > 0.0, "a moving actor's tone disappeared");
                assert!(maximum / minimum < 1.001, "a moving actor's tone changed volume");
            }
            assert!(ranges[0].0 > ranges[1].1 * 3.0);
            assert!(ranges[0].0 > ranges[2].1 * 3.0);
        }
    }
}

#[test]
fn loop_and_regular_audio_load_the_same_file_through_their_own_loaders() {
    let directory = tempfile::tempdir().expect("audio fixture directory unavailable");
    let bytes = wav(&[-16384, 16384], 1, 44100);
    fs::write(directory.path().join("tone.wav"), &bytes).expect("audio fixture write failed");
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin {
            file_path: directory.path().to_string_lossy().into_owned(),
            ..default()
        },
    ))
    .init_asset::<AudioSource>()
    .init_asset_loader::<AudioLoader>()
    .init_asset::<LoopAudio>()
    .init_asset_loader::<LoopAudioLoader>();
    let assets = app.world().resource::<AssetServer>();
    let regular = assets.load::<AudioSource>("tone.wav");
    let looping = assets.load::<LoopAudio>("tone.wav");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        if let (Some(regular), Some(looping)) = (
            app.world().resource::<Assets<AudioSource>>().get(&regular),
            app.world().resource::<Assets<LoopAudio>>().get(&looping),
        ) {
            assert_eq!(regular.bytes.as_ref(), bytes);
            assert_eq!(looping.samples.as_ref(), [-0.5, 0.5]);
            break;
        }
        assert!(Instant::now() < deadline, "audio fixture assets did not load");
        thread::sleep(Duration::from_millis(1));
    }
}
