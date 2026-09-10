use super::*;
use crate::test_fixtures;
use common::protocol::Position;

fn planned_move(index: u32, start: Position, target: Position) -> CharacterMovePlan {
    let entity = Entity::from_raw_u32(index).expect("test entity index out of range");
    let physics = test_fixtures::gameplay_config().player.physics();
    CharacterMovePlan::from_target(entity, start, target, 0.0, physics, false)
}

#[test]
fn overlapping_planned_characters_can_separate() {
    let first = planned_move(
        1,
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position {
            x: -0.2,
            y: 0.0,
            z: 0.0,
        },
    );
    let second = planned_move(
        2,
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 1.0, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlapping_character(&first, &planned_moves).is_none());
    assert!(overlapping_character(&second, &planned_moves).is_none());
}

#[test]
fn overlapping_planned_characters_cannot_move_deeper_together() {
    let first = planned_move(
        1,
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position { x: 0.2, y: 0.0, z: 0.0 },
    );
    let second = planned_move(
        2,
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 0.6, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlapping_character(&first, &planned_moves).is_some());
    assert!(overlapping_character(&second, &planned_moves).is_some());
}
