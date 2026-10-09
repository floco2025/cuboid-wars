use super::*;

#[test]
fn missile_movement_state_velocity_round_trips() {
    let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
    let velocity = Vec3::new(3.0, -4.0, 12.0);
    let state = MissileMovementState::from_velocity(pos, velocity);
    assert!((state.velocity() - velocity).length() < 1e-4);
    assert!((state.speed - 13.0).abs() < 1e-4);

    let stationary = MissileMovementState::from_velocity(pos, Vec3::ZERO);
    assert_eq!(stationary.speed, 0.0);
    assert_eq!(stationary.velocity(), Vec3::ZERO);
}
