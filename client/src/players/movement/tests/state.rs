use super::*;

#[test]
fn movement_state_round_trips_through_its_components() {
    let state = PlayerMovementState {
        carrier: CarrierId::WORLD,
        pos: Position { x: 1.0, y: 2.0, z: 3.0 },
        move_intent: PlayerMoveIntent::Running { direction: 0.5 },
        vertical_velocity: -4.0,
        face_yaw: 1.25,
        airborne_momentum: [1.0, 0.0, -2.0],
        knockback: [0.5, 0.0, 0.25],
        support: CharacterSupport::Ladder,
    };
    let bundle = PlayerMotionBundle::from(&state);
    let rebuilt = player_movement_state(
        state.pos,
        bundle.move_intent,
        &bundle.face_yaw,
        &bundle.vertical_velocity,
        &bundle.airborne_momentum,
        &bundle.knockback,
        bundle.support,
    );
    assert_eq!(rebuilt.pos, state.pos);
    assert_eq!(rebuilt.move_intent, state.move_intent);
    assert_eq!(rebuilt.vertical_velocity, state.vertical_velocity);
    assert_eq!(rebuilt.face_yaw, state.face_yaw);
    assert_eq!(rebuilt.airborne_momentum, state.airborne_momentum);
    assert_eq!(rebuilt.knockback, state.knockback);
    assert_eq!(rebuilt.support, state.support);
}
