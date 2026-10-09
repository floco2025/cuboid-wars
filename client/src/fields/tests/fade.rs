use super::*;

const OPACITY: f32 = 0.8;
const PASSABLE_OPACITY: f32 = 0.15;
const FADE_SECS: f32 = 0.25;

#[test]
fn fade_step_approaches_and_settles_then_stops_writing() {
    let mut alpha = PASSABLE_OPACITY;
    let first = fade_step(alpha, OPACITY, 0.05, FADE_SECS).expect("first step reports settled");
    assert!(first > alpha && first < OPACITY);
    alpha = first;
    for _ in 0..200 {
        match fade_step(alpha, OPACITY, 0.05, FADE_SECS) {
            Some(next) => alpha = next,
            None => break,
        }
    }
    assert_eq!(alpha, OPACITY);
    assert_eq!(fade_step(alpha, OPACITY, 0.05, FADE_SECS), None);
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
