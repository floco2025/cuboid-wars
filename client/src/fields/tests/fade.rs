use super::*;

const CONFIG: FieldVfxConfig = FieldVfxConfig {
    emissive_brightness: 1.0,
    rail_emissive_brightness: 0.5,
    opacity: 0.8,
    passable_opacity: 0.15,
    fade_secs: 0.25,
};

#[test]
fn fade_targets_follow_the_open_fields() {
    let switch_state = SwitchState {
        open_fields: vec![FieldId(0), FieldId(3)],
        ..Default::default()
    };
    for (field, opacity) in [
        (FieldId(3), CONFIG.passable_opacity),
        (FieldId(0), CONFIG.passable_opacity),
        (FieldId(1), CONFIG.opacity),
    ] {
        assert_eq!(fade_target(&switch_state, field, CONFIG), opacity);
    }
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
fn forgetting_one_kind_of_piece_keeps_the_other() {
    let surface = |piece| FieldSurface {
        field: FieldId(0),
        piece,
        material: Handle::default(),
        base_color: Color::WHITE,
    };
    let mut surfaces = FieldSurfaces(vec![surface(FieldPiece::Barrier), surface(FieldPiece::Bridge)]);
    surfaces.forget(FieldPiece::Barrier);
    assert_eq!(
        surfaces.0.iter().map(|surface| surface.piece).collect::<Vec<_>>(),
        [FieldPiece::Bridge]
    );
    surfaces.forget(FieldPiece::Bridge);
    assert!(surfaces.0.is_empty());
}
