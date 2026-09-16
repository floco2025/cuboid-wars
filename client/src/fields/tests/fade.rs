use super::*;
use common::protocol::{BarrierId, BridgeId};

const CONFIG: FieldVfxConfig = FieldVfxConfig {
    emissive_brightness: 1.0,
    rail_emissive_brightness: 0.5,
    opacity: 0.8,
    passable_opacity: 0.15,
    fade_secs: 0.25,
};

#[test]
fn fade_targets_follow_open_barriers_and_powered_bridges() {
    let plates = PlateState {
        open_barriers: vec![BarrierId(3)],
        powered_bridges: vec![BridgeId(1)],
        ..Default::default()
    };
    assert_eq!(
        fade_target(&plates, FieldKind::Barrier(BarrierId(3)), CONFIG),
        CONFIG.passable_opacity
    );
    assert_eq!(
        fade_target(&plates, FieldKind::Barrier(BarrierId(0)), CONFIG),
        CONFIG.opacity
    );
    assert_eq!(
        fade_target(&plates, FieldKind::Bridge(BridgeId(1)), CONFIG),
        CONFIG.opacity
    );
    assert_eq!(
        fade_target(&plates, FieldKind::Bridge(BridgeId(0)), CONFIG),
        CONFIG.passable_opacity
    );
}

#[test]
fn fade_step_approaches_and_settles_then_stops_writing() {
    let mut alpha = CONFIG.passable_opacity;
    let first = fade_step(alpha, CONFIG.opacity, 0.05, CONFIG.fade_secs).expect("first step reports settled");
    assert!(first > alpha && first < CONFIG.opacity);
    alpha = first;
    for _ in 0..200 {
        match fade_step(alpha, CONFIG.opacity, 0.05, CONFIG.fade_secs) {
            Some(next) => alpha = next,
            None => break,
        }
    }
    assert_eq!(alpha, CONFIG.opacity);
    assert_eq!(fade_step(alpha, CONFIG.opacity, 0.05, CONFIG.fade_secs), None);
}

#[test]
fn fade_duration_controls_how_quickly_opacity_changes() {
    let fast = fade_step(0.1, 0.9, 0.1, 0.1).expect("fast fade reports settled");
    let slow = fade_step(0.1, 0.9, 0.1, 1.0).expect("slow fade reports settled");
    assert!(fast > slow);
    assert!((fast - (0.1 + 0.8 * (1.0 - (-1.0_f32).exp()))).abs() < 1e-6);
}

#[test]
fn forgetting_one_domain_keeps_the_other() {
    let surface = |state| FieldSurface {
        state,
        material: Handle::default(),
        base_color: Color::WHITE,
    };
    let mut surfaces = FieldSurfaces(vec![
        surface(FieldKind::Barrier(BarrierId(0))),
        surface(FieldKind::Bridge(BridgeId(0))),
    ]);
    surfaces.forget_barriers();
    assert!(matches!(
        surfaces.0.as_slice(),
        [FieldSurface {
            state: FieldKind::Bridge(_),
            ..
        }]
    ));
    surfaces.forget_bridges();
    assert!(surfaces.0.is_empty());
}
