use super::*;

const fn level(level: u8, span: u8) -> MapLevel {
    MapLevel { level, span }
}

#[test]
fn visibility_combines_open_kind_and_level_focus() {
    let kind = BarrierKindId(2);

    assert_eq!(
        barrier_visibility(&[kind], FocusedMapLevel(Some(1)), kind, level(1, 0)),
        Visibility::Hidden
    );
    assert_eq!(
        barrier_visibility(&[], FocusedMapLevel(Some(2)), kind, level(1, 0)),
        Visibility::Hidden
    );
    assert_eq!(
        barrier_visibility(&[], FocusedMapLevel(Some(1)), kind, level(1, 0)),
        Visibility::Visible
    );
}

#[test]
fn a_stacked_barrier_shows_on_every_storey_it_spans() {
    let kind = BarrierKindId(0);

    assert_eq!(
        barrier_visibility(&[], FocusedMapLevel(Some(2)), kind, level(1, 1)),
        Visibility::Visible
    );
    assert_eq!(
        barrier_visibility(&[], FocusedMapLevel(Some(3)), kind, level(1, 1)),
        Visibility::Hidden
    );
    assert_eq!(
        barrier_visibility(&[], FocusedMapLevel(Some(0)), kind, level(1, 1)),
        Visibility::Hidden
    );
}
