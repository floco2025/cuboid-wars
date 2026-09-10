use super::*;
use std::f32::consts::FRAC_PI_2;

#[test]
fn fade_progress_clamps_to_the_warning_window() {
    let ghost = BeamInGhost {
        reserved_tick: 100,
        due_tick: 190,
        half_extents: Vec3::ONE,
        center_height: 0.0,
    };
    assert_eq!(ghost.fade_progress(100, 0.0), 0.0);
    assert_eq!(ghost.fade_progress(145, 0.0), 0.5);
    assert!((ghost.fade_progress(144, 0.5) - 44.5 / 90.0).abs() < 1e-6);
    assert_eq!(ghost.fade_progress(190, 0.0), 1.0);
    assert_eq!(ghost.fade_progress(99, 0.9), 0.0);
    assert_eq!(ghost.fade_progress(250, 0.0), 1.0);
    assert!(!ghost.is_due(189));
    assert!(ghost.is_due(190));
}

#[test]
fn missed_emissions_are_capped_without_building_debt() {
    let mut credit = 0.0;
    assert_eq!(take_emissions(&mut credit, 1_000.0, 1.0, 32), 32);
    assert_eq!(take_emissions(&mut credit, 0.0, 0.0, 32), 0);
}

#[test]
fn sparkle_offsets_follow_ghost_rotation() {
    let transform = GlobalTransform::from(
        Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2)).with_translation(Vec3::new(3.0, 2.0, 1.0)),
    );
    let world = sparkle_world_position(&transform, Vec3::X);
    assert!(world.abs_diff_eq(Vec3::new(3.0, 2.0, 0.0), 0.0001));
}

#[test]
fn sparkle_density_scales_the_small_actor_floor() {
    assert_eq!(sparkle_rate(0.01, 200.0), 20.0);
    assert_eq!(sparkle_rate(0.01, 100.0), 10.0);
    assert_eq!(sparkle_rate(0.01, 0.0), 0.0);
}
