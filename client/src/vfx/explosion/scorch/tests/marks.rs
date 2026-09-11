use super::*;

#[test]
fn scorch_alpha_stays_opaque_then_fades_to_zero() {
    let full_opacity_duration = EXPLOSION_SCORCH_FULL_OPACITY_SECS;
    let fade_duration = scorch_fade_duration(full_opacity_duration);
    assert_eq!(scorch_alpha(0.0, full_opacity_duration), 1.0);
    assert_eq!(scorch_alpha(full_opacity_duration, full_opacity_duration), 1.0);
    assert_eq!(
        scorch_alpha(full_opacity_duration + fade_duration / 2.0, full_opacity_duration),
        0.5
    );
    assert_eq!(
        scorch_alpha(full_opacity_duration + fade_duration, full_opacity_duration),
        0.0
    );
}

#[test]
fn grass_burn_intensity_tracks_scorch_fade_in_bounded_steps() {
    let step = 1.0 / GRASS_BURN_FADE_STEPS as f32;
    assert_eq!(grass_burn_intensity(1.0), 1.0);
    assert_eq!(grass_burn_intensity(0.5), 0.5);
    assert_eq!(grass_burn_intensity(step * 0.9), 0.0);
    assert_eq!(grass_burn_intensity(-1.0), 0.0);
    assert_eq!(grass_burn_intensity(2.0), 1.0);
}
