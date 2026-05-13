use super::*;

#[test]
fn overlapping_planned_characters_can_separate() {
    let first = planned_move(
        test_entity(1),
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position {
            x: -0.2,
            y: 0.0,
            z: 0.0,
        },
    );
    let second = planned_move(
        test_entity(2),
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 1.0, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(!overlaps_other_character(&first, &planned_moves));
    assert!(!overlaps_other_character(&second, &planned_moves));
}

#[test]
fn overlapping_planned_characters_cannot_move_deeper_together() {
    let first = planned_move(
        test_entity(1),
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position { x: 0.2, y: 0.0, z: 0.0 },
    );
    let second = planned_move(
        test_entity(2),
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 0.6, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlaps_other_character(&first, &planned_moves));
    assert!(overlaps_other_character(&second, &planned_moves));
}

#[test]
fn item_overlap_uses_vertical_distance() {
    let player = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    let item = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert!(!character_overlaps_item(&player, &item, 1.0));
}

#[test]
fn item_overlap_allows_same_level_collection() {
    let player = Position {
        x: 0.25,
        y: 0.0,
        z: 0.25,
    };
    let item = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert!(character_overlaps_item(&player, &item, 1.0));
}
