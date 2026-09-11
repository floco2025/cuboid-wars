use super::*;

#[test]
fn fade_targets_follow_the_powered_kinds() {
    let plates = PlateState {
        powered_bridges: vec![BridgeId(1)],
        ..Default::default()
    };
    let config = LightBridgeVfxConfig {
        emissive_brightness: 1.0,
        opacity: 0.6,
        unpowered_opacity: 0.1,
        fade_secs: 0.25,
    };
    assert_eq!(fade_target(&plates, BridgeId(1), config), config.opacity);
    assert_eq!(fade_target(&plates, BridgeId(0), config), config.unpowered_opacity);
}

#[test]
fn fade_step_approaches_and_settles_then_stops_writing() {
    let config = LightBridgeVfxConfig {
        emissive_brightness: 1.0,
        opacity: 0.8,
        unpowered_opacity: 0.15,
        fade_secs: 0.25,
    };
    let mut alpha = config.unpowered_opacity;
    let first = fade_step(alpha, config.opacity, 0.05, config.fade_secs).expect("first step reports settled");
    assert!(first > alpha && first < config.opacity);
    alpha = first;
    for _ in 0..200 {
        match fade_step(alpha, config.opacity, 0.05, config.fade_secs) {
            Some(next) => alpha = next,
            None => break,
        }
    }
    assert_eq!(alpha, config.opacity);
    assert_eq!(fade_step(alpha, config.opacity, 0.05, config.fade_secs), None);
}

#[test]
fn fade_duration_controls_how_quickly_opacity_changes() {
    let fast = fade_step(0.1, 0.9, 0.1, 0.1).expect("fast fade reports settled");
    let slow = fade_step(0.1, 0.9, 0.1, 1.0).expect("slow fade reports settled");
    assert!(fast > slow);
    assert!((fast - (0.1 + 0.8 * (1.0 - (-1.0_f32).exp()))).abs() < 1e-6);
}
