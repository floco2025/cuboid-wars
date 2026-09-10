use super::*;

fn audio() -> AudioConfig {
    AudioConfig {
        spatial_distance_scale: 0.1,
        explosion_gain: 2.0,
        rain_volume: 1.0,
        bump: BumpAudioConfig {
            min_run_up: 2.0,
            full_run_up: 6.0,
        },
    }
}

#[test]
fn bump_volume_is_silent_below_the_threshold_and_ramps_to_full() {
    let bump = BumpAudioConfig {
        min_run_up: 2.0,
        full_run_up: 6.0,
    };
    assert_eq!(bump.volume_for(1.9), None);
    assert_eq!(bump.volume_for(2.0), Some(0.0));
    assert!((bump.volume_for(4.0).expect("mid run-up silent") - 0.5).abs() < 1e-6);
    assert_eq!(bump.volume_for(6.0), Some(1.0));
    assert_eq!(bump.volume_for(20.0), Some(1.0));
}

#[test]
fn bump_config_rejects_a_full_run_up_at_or_below_the_threshold() {
    let config = AudioConfig {
        bump: BumpAudioConfig {
            min_run_up: 3.0,
            full_run_up: 3.0,
        },
        ..audio()
    };
    let error = config.validate().expect_err("flat ramp accepted");
    assert!(error.to_string().contains("audio.bump.full_run_up"), "{error}");
}

#[test]
fn audio_config_rejects_zero_spatial_distance_scale() {
    let config = AudioConfig {
        spatial_distance_scale: 0.0,
        ..audio()
    };
    let error = config.validate().expect_err("zero spatial distance scale should fail");
    assert!(error.to_string().contains("spatial_distance_scale"));
}
