use super::*;

#[test]
fn wall_cross_section_stops_at_reach_limit() {
    assert!(wall_scorch_diameter(2.0, 1.21, 0.6).is_none());
    assert!(wall_scorch_diameter(2.0, 1.20, 0.6).is_some());
}

#[test]
fn surface_cross_section_shrinks_with_distance() {
    assert_eq!(surface_cross_section_diameter(2.0, 0.0), Some(4.0));
    assert_eq!(surface_cross_section_diameter(2.0, 2.0), None);
    let diameter = surface_cross_section_diameter(2.0, 1.0).expect("surface intersects scorch volume");
    assert!((diameter - 2.0 * 3.0_f32.sqrt()).abs() < 0.001);
}

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
