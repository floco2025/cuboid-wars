use super::*;

#[test]
fn grass_burn_intensity_tracks_scorch_fade_in_bounded_steps() {
    let step = 1.0 / GRASS_BURN_FADE_STEPS as f32;
    assert_eq!(grass_burn_intensity(1.0), 1.0);
    assert_eq!(grass_burn_intensity(0.5), 0.5);
    assert_eq!(grass_burn_intensity(step * 0.9), 0.0);
    assert_eq!(grass_burn_intensity(-1.0), 0.0);
    assert_eq!(grass_burn_intensity(2.0), 1.0);
}
