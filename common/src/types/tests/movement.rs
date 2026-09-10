use super::*;

#[test]
fn move_intents_reject_only_non_finite_directions() {
    assert!(PlayerMoveIntent::Idle.is_finite());
    for (direction, finite) in [
        (0.0, true),
        (-2.5, true),
        (f32::NAN, false),
        (f32::INFINITY, false),
        (f32::NEG_INFINITY, false),
    ] {
        for intent in [
            PlayerMoveIntent::Walking { direction },
            PlayerMoveIntent::Running { direction },
        ] {
            assert_eq!(intent.is_finite(), finite, "{intent:?}");
        }
    }
}

#[test]
fn missile_movement_state_velocity_round_trips() {
    let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
    let velocity = Vec3::new(3.0, -4.0, 12.0);
    let state = MissileMovementState::from_velocity(pos, velocity);
    let recovered = state.velocity();
    assert!((recovered - velocity).length() < 1e-4);
    assert!((state.speed - 13.0).abs() < 1e-4);
}

#[test]
fn missile_movement_state_zero_velocity_is_stationary() {
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let state = MissileMovementState::from_velocity(pos, Vec3::ZERO);
    assert_eq!(state.speed, 0.0);
    assert_eq!(state.velocity(), Vec3::ZERO);
}
